//! Loaded VST3 plugin instance and core plugin behavior.

use notwig_rack_core::bus::BusLayout;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::info::{ParameterInfo, PluginInfo, PresetInfo};
use notwig_rack_core::plugin::PluginCore;

use std::ptr;

use vst3::Steinberg::Vst::{
    IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, ProcessModes_, ProcessSetup,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    IBStream, IPlugView, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, TUID, kResultOk,
    kResultTrue,
};
use vst3::{ComPtr, ComWrapper};

use crate::editor::create_editor_view;
use crate::module::LoadedModule;
use crate::params::{empty_parameter_info, vst3_param_info_to_rack};
use crate::scanner::hex_to_tuid;
use crate::stream::MemoryStream;

/// Stereo speaker arrangement = `kSpeakerL | kSpeakerR`. Defined
/// here to avoid a `kSpeaker*` import dance.
const STEREO_ARRANGEMENT: u64 = 0x03;

/// One loaded VST3 plugin instance.
///
/// Holds three COM smart pointers — component, audio processor,
/// edit controller — plus the dlopen handle that keeps the
/// underlying dylib mapped. When `Drop`s, the COM pointers
/// release their objects which triggers `terminate()` and
/// component disposal.
pub struct Vst3Plugin {
    info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,

    component: ComPtr<IComponent>,
    pub(crate) processor: ComPtr<IAudioProcessor>,
    pub(crate) controller: ComPtr<IEditController>,
    /// `true` when controller and component are *different* COM
    /// objects (separate-controller architecture) and we've wired
    /// their connection points.
    separate_controller: bool,
    component_cp: Option<ComPtr<IConnectionPoint>>,
    controller_cp: Option<ComPtr<IConnectionPoint>>,

    param_count: usize,
    processing: bool,
    /// Parameter changes queued for the next process call. VST3 hosts must
    /// deliver these through `ProcessData::inputParameterChanges` so a
    /// separate edit controller's changes reach the audio processor.
    pub(crate) pending_params: Vec<(u32, f64)>,

    /// Cached `IPlugView` for the plugin's editor. Created on
    /// `open()`, released on `close()` / Drop.
    pub(crate) view: Option<ComPtr<IPlugView>>,
    pub(crate) editor_open: bool,

    // Declared last so all COM objects and the view are released before
    // `bundleExit` runs when the struct is dropped.
    _module: LoadedModule,
}

impl Vst3Plugin {
    pub(crate) fn load_from(info: &PluginInfo) -> Result<Self> {
        let module = unsafe { LoadedModule::open(&info.path) }?;
        let factory = module.factory()?;

        let cid = hex_to_tuid(&info.unique_id).ok_or_else(|| Error::LoadFailed {
            path: info.path.clone(),
            reason: format!("could not parse VST3 unique_id {:?}", info.unique_id),
        })?;

        // Create IComponent.
        let component_ptr =
            unsafe { create_instance::<IComponent>(&factory, &cid) }.ok_or_else(|| {
                Error::LoadFailed {
                    path: info.path.clone(),
                    reason: "factory.createInstance(IComponent) returned NULL".into(),
                }
            })?;
        let component = component_ptr;

        // Initialize the component. Many plugins accept a NULL
        // context (hosts only need to supply IHostApplication for
        // plugins that look it up via queryInterface).
        if unsafe { component.initialize(ptr::null_mut()) } != kResultOk {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: "IComponent::initialize returned non-OK".into(),
            });
        }

        // Cast to IAudioProcessor.
        let processor = component
            .as_com_ref()
            .cast::<IAudioProcessor>()
            .ok_or_else(|| Error::LoadFailed {
                path: info.path.clone(),
                reason: "component does not implement IAudioProcessor".into(),
            })?;

        // Look up the separate-controller class id; if present we
        // create a separate controller and connect to it. Otherwise
        // the component is itself the controller.
        let mut controller_cid: TUID = [0; 16];
        let cls_id_status = unsafe { component.getControllerClassId(&raw mut controller_cid) };
        let (controller, separate_controller) = if cls_id_status == kResultTrue {
            let ctrl_ptr = unsafe { create_instance::<IEditController>(&factory, &controller_cid) }
                .ok_or_else(|| Error::LoadFailed {
                    path: info.path.clone(),
                    reason: "factory.createInstance(IEditController) returned NULL".into(),
                })?;
            if unsafe { ctrl_ptr.initialize(ptr::null_mut()) } != kResultOk {
                return Err(Error::LoadFailed {
                    path: info.path.clone(),
                    reason: "IEditController::initialize returned non-OK".into(),
                });
            }
            (ctrl_ptr, true)
        } else {
            // Single-component plugin — controller and component
            // share an object. Cast through queryInterface so the
            // refcount is correct.
            let ctrl = component
                .as_com_ref()
                .cast::<IEditController>()
                .ok_or_else(|| Error::LoadFailed {
                    path: info.path.clone(),
                    reason:
                        "component is not its own controller and didn't report a controller cid"
                            .into(),
                })?;
            (ctrl, false)
        };

        // Optional: connect the two connection points so the
        // plugin's component and controller can talk to each
        // other (param/audio synchronisation). Best-effort —
        // some plugins skip these even when the controller is
        // separate.
        let (component_cp, controller_cp) = if separate_controller {
            let cp_a = component.as_com_ref().cast::<IConnectionPoint>();
            let cp_b = controller.as_com_ref().cast::<IConnectionPoint>();
            if let (Some(a), Some(b)) = (&cp_a, &cp_b) {
                unsafe {
                    a.connect(b.as_com_ref().as_ptr().cast());
                    b.connect(a.as_com_ref().as_ptr().cast());
                }
            }
            (cp_a, cp_b)
        } else {
            (None, None)
        };

        let param_count_raw = unsafe { controller.getParameterCount() }.max(0);
        #[allow(clippy::cast_sign_loss)]
        let param_count = param_count_raw as usize;

        let mut info = info.clone();
        // ARA requires binding before the companion creates any view. Probing
        // the editor here would violate that ordering, so ARA-capable VST3
        // processors are reported as having an editor without creating one.
        #[cfg(feature = "ara")]
        let ara_supported = crate::ara::factory(&component).is_some();
        #[cfg(not(feature = "ara"))]
        let ara_supported = info.supports_ara;
        if ara_supported {
            info.has_editor = true;
        } else {
            // The editor exists if `createView("editor")` returns a
            // non-null view. We probe by creating + releasing it once
            // at load time. (Some plugins are slow to create the view;
            // a heavier-weight host would defer this to first open.)
            info.has_editor = unsafe { create_editor_view(&controller) }.is_some();
        }
        #[cfg(feature = "ara")]
        {
            info.supports_ara = ara_supported;
        }

        Ok(Self {
            info,
            layouts: vec![BusLayout::stereo()],
            active_layout: None,
            _module: module,
            component,
            processor,
            controller,
            separate_controller,
            component_cp,
            controller_cp,
            param_count,
            processing: false,
            pending_params: Vec::new(),
            view: None,
            editor_open: false,
        })
    }
}

/// Run `factory.createInstance` for `I` and wrap the result as a
/// `ComPtr<I>`. The factory call uses the interface's `IID`
/// directly so the plugin returns the correct `*mut c_void`.
unsafe fn create_instance<I>(factory: &ComPtr<IPluginFactory>, cid: &TUID) -> Option<ComPtr<I>>
where
    I: vst3::Interface,
{
    let mut obj: *mut std::ffi::c_void = ptr::null_mut();
    // `com_scrape_types::Guid` and `TUID` share the same
    // `[i8; 16]` layout — reinterpret to avoid an extra copy.
    let iid_bytes: &TUID =
        unsafe { &*(std::ptr::from_ref::<vst3::com_scrape_types::Guid>(&I::IID).cast::<TUID>()) };
    let cid_ptr = cid.as_ptr();
    let iid_ptr = iid_bytes.as_ptr();
    if unsafe { factory.createInstance(cid_ptr, iid_ptr, &raw mut obj) } != kResultOk
        || obj.is_null()
    {
        return None;
    }
    unsafe { ComPtr::<I>::from_raw(obj.cast()) }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        // Remove the plugin view before terminating the component; live views
        // call back into the controller during teardown.
        if self.editor_open {
            notwig_rack_core::editor::PluginEditor::close(self);
        }
        if self.processing {
            unsafe { self.processor.setProcessing(0) };
        }
        if self.active_layout.is_some() {
            unsafe { self.component.setActive(0) };
        }
        if let (Some(a), Some(b)) = (&self.component_cp, &self.controller_cp) {
            unsafe {
                a.disconnect(b.as_com_ref().as_ptr().cast());
                b.disconnect(a.as_com_ref().as_ptr().cast());
            }
        }
        if self.separate_controller {
            unsafe { self.controller.terminate() };
        }
        unsafe { self.component.terminate() };
    }
}

impl PluginCore for Vst3Plugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }
    fn active_layout(&self) -> Option<&BusLayout> {
        self.active_layout.as_ref()
    }
    fn supported_layouts(&self) -> &[BusLayout] {
        &self.layouts
    }

    fn parameter_count(&self) -> usize {
        self.param_count
    }

    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        if index >= self.param_count {
            return Err(Error::InvalidParameter(index));
        }
        let mut info = empty_parameter_info();
        let i32_index = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        if unsafe { self.controller.getParameterInfo(i32_index, &raw mut info) } != kResultOk {
            return Err(Error::InvalidParameter(index));
        }
        Ok(vst3_param_info_to_rack(&info))
    }

    fn parameter_value(&self, index: usize) -> Result<f64> {
        if index >= self.param_count {
            return Err(Error::InvalidParameter(index));
        }
        let mut info = empty_parameter_info();
        let i32_index = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        if unsafe { self.controller.getParameterInfo(i32_index, &raw mut info) } != kResultOk {
            return Err(Error::InvalidParameter(index));
        }
        Ok(unsafe { self.controller.getParamNormalized(info.id) })
    }

    fn parameter_value_string(&self, index: usize, _value: f64) -> Result<String> {
        // VST3 exposes IEditController::getParamStringByValue —
        // wiring it requires a TChar (UTF-16) buffer round-trip.
        // Tracked as a follow-on to avoid pulling in a wide-char
        // dep here.
        let _ = index;
        Err(Error::Other("vst3 parameter_value_string TODO".into()))
    }

    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        if index >= self.param_count {
            return Err(Error::InvalidParameter(index));
        }
        let mut info = empty_parameter_info();
        let i32_index = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        if unsafe { self.controller.getParameterInfo(i32_index, &raw mut info) } != kResultOk {
            return Err(Error::InvalidParameter(index));
        }
        let clamped = value.clamp(0.0, 1.0);
        if unsafe { self.controller.setParamNormalized(info.id, clamped) } != kResultOk {
            return Err(Error::Other(
                "IEditController::setParamNormalized failed".into(),
            ));
        }
        if let Some(existing) = self
            .pending_params
            .iter_mut()
            .find(|(id, _)| *id == info.id)
        {
            existing.1 = clamped;
        } else {
            self.pending_params.push((info.id, clamped));
        }
        Ok(())
    }

    fn preset_count(&self) -> usize {
        // VST3 exposes presets via the `IUnitInfo` interface,
        // wired in a follow-on. Treat as zero for now.
        0
    }
    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        Err(Error::InvalidParameter(index))
    }
    fn load_preset(&mut self, _preset_number: i32) -> Result<()> {
        Err(Error::Other("vst3 preset loading not yet wired".into()))
    }

    fn save_state(&self) -> Result<Vec<u8>> {
        let stream = ComWrapper::new(MemoryStream::default());
        let stream_ptr = stream
            .to_com_ptr::<IBStream>()
            .ok_or_else(|| Error::Other("MemoryStream missing IBStream IID".into()))?;
        let status = unsafe { self.component.getState(stream_ptr.as_ptr()) };
        if status != kResultOk {
            return Err(Error::Other(format!(
                "IComponent::getState returned {status}"
            )));
        }
        Ok(stream.data.borrow().clone())
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        let stream = ComWrapper::new(MemoryStream {
            data: std::cell::RefCell::new(bytes.to_vec()),
            position: std::cell::Cell::new(0),
        });
        let stream_ptr = stream
            .to_com_ptr::<IBStream>()
            .ok_or_else(|| Error::Other("MemoryStream missing IBStream IID".into()))?;
        let status = unsafe { self.component.setState(stream_ptr.as_ptr()) };
        if status != kResultOk {
            return Err(Error::Other(format!(
                "IComponent::setState returned {status}"
            )));
        }
        Ok(())
    }

    fn activate(
        &mut self,
        layout: BusLayout,
        sample_rate: f64,
        max_block_size: usize,
    ) -> Result<()> {
        let mut input_arr = STEREO_ARRANGEMENT;
        let mut output_arr = STEREO_ARRANGEMENT;
        let _ = unsafe {
            self.processor
                .setBusArrangements(&raw mut input_arr, 1, &raw mut output_arr, 1)
        };

        let mut setup = ProcessSetup {
            #[allow(clippy::cast_possible_wrap)]
            processMode: ProcessModes_::kRealtime as i32,
            #[allow(clippy::cast_possible_wrap)]
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: i32::try_from(max_block_size).unwrap_or(i32::MAX),
            sampleRate: sample_rate,
        };
        if unsafe { self.processor.setupProcessing(&raw mut setup) } != kResultOk {
            return Err(Error::Other(
                "IAudioProcessor::setupProcessing failed".into(),
            ));
        }
        if unsafe { self.component.setActive(1) } != kResultOk {
            return Err(Error::Other("IComponent::setActive(true) failed".into()));
        }
        if unsafe { self.processor.setProcessing(1) } != kResultOk {
            return Err(Error::Other(
                "IAudioProcessor::setProcessing(true) failed".into(),
            ));
        }
        self.processing = true;
        self.active_layout = Some(layout);
        Ok(())
    }

    fn deactivate(&mut self) {
        self.pending_params.clear();
        if self.processing {
            unsafe { self.processor.setProcessing(0) };
            self.processing = false;
        }
        if self.active_layout.is_some() {
            unsafe { self.component.setActive(0) };
        }
        self.active_layout = None;
    }
    fn is_active(&self) -> bool {
        self.active_layout.is_some()
    }

    fn latency_samples(&self) -> u32 {
        unsafe { self.processor.getLatencySamples() }
    }

    fn editor(&mut self) -> Option<&mut dyn notwig_rack_core::editor::PluginEditor> {
        if !self.info.has_editor {
            return None;
        }
        Some(self)
    }
}

#[cfg(feature = "ara")]
impl notwig_rack_ara::AraPlugin for Vst3Plugin {
    fn ara_factory(&self) -> Option<notwig_rack_ara::AraFactory> {
        let raw = crate::ara::factory(&self.component)?;
        unsafe { notwig_rack_ara::AraFactory::from_raw(raw).ok() }
    }

    unsafe fn bind_ara(
        &mut self,
        controller: notwig_rack_ara::ARADocumentControllerRef,
        known: notwig_rack_ara::AraRoles,
        assigned: notwig_rack_ara::AraRoles,
    ) -> std::result::Result<notwig_rack_ara::AraPluginExtension, notwig_rack_ara::AraError> {
        crate::ara::bind(&self.component, controller, known, assigned)
    }
}
