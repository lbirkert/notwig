use notwig_rack_core::bus::BusLayout;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::info::{ParameterInfo, PluginInfo, PresetInfo};
use notwig_rack_core::plugin::PluginCore;

use objc2_audio_toolbox::{
    AUPreset, AURenderCallbackStruct, AudioComponent, AudioComponentDescription,
    AudioComponentFindNext, AudioComponentFlags, AudioComponentInstance,
    AudioComponentInstanceDispose, AudioComponentInstanceNew, AudioComponentInstantiate,
    AudioComponentInstantiationOptions, AudioUnitGetParameter, AudioUnitGetProperty,
    AudioUnitInitialize, AudioUnitParameterID, AudioUnitSetParameter, AudioUnitSetProperty,
    AudioUnitUninitialize, HostCallbackInfo, kAudioUnitProperty_ClassInfo,
    kAudioUnitProperty_HostCallbacks, kAudioUnitProperty_Latency,
    kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitProperty_PresentPreset,
    kAudioUnitProperty_SetRenderCallback, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global,
    kAudioUnitScope_Input, kAudioUnitScope_Output,
};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved,
    kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
};

use std::ptr;

use crate::editor::{AuEditorState, has_cocoa_ui};
use crate::params::{
    au_parameter_to_rack, fetch_factory_presets, fetch_parameter_ids, fetch_parameter_info,
};
use crate::render::{RenderContext, alloc_audio_buffer_list, render_input_callback};
use crate::scan::parse_unique_id;
use crate::transport::{
    HostTransport, host_beat_and_tempo, host_musical_time, host_transport_state,
    host_transport_state2,
};

/// Synchronous `AudioComponentInstanceNew` wrapper. Used for AU v2
/// and for AU v3 plugins that don't set `RequiresAsyncInstantiation`.
fn instantiate_sync(
    component: AudioComponent,
    info_path: &std::path::Path,
) -> Result<AudioComponentInstance> {
    let mut instance: AudioComponentInstance = ptr::null_mut();
    let status = unsafe {
        AudioComponentInstanceNew(component, ptr::NonNull::new_unchecked(&raw mut instance))
    };
    if status != 0 || instance.is_null() {
        return Err(Error::LoadFailed {
            path: info_path.to_path_buf(),
            reason: format!("AudioComponentInstanceNew failed with OSStatus {status}"),
        });
    }
    Ok(instance)
}

/// Asynchronous instantiation via `AudioComponentInstantiate`,
/// required for AU v3 plugins flagged `RequiresAsyncInstantiation`
/// (sandboxed App-Extension-hosted units). The completion block
/// can be dispatched from any thread; we pump the main thread's
/// `CFRunLoop` until the result lands or a 10-second deadline
/// elapses.
fn instantiate_async(
    component: AudioComponent,
    info_path: &std::path::Path,
) -> Result<AudioComponentInstance> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
    use std::time::{Duration, Instant};

    let done = Arc::new(AtomicBool::new(false));
    let inst_ptr: Arc<AtomicPtr<std::ffi::c_void>> = Arc::new(AtomicPtr::new(ptr::null_mut()));
    let status_atom = Arc::new(AtomicI32::new(0));

    let done_clone = Arc::clone(&done);
    let inst_clone = Arc::clone(&inst_ptr);
    let status_clone = Arc::clone(&status_atom);

    // RcBlock heap-allocates the block and refcounts via Apple's
    // blocks runtime; AudioComponentInstantiate retains it for the
    // duration of the call, releases after firing. As long as our
    // Arc holders outlive the block (which we ensure by holding
    // them until `done == true`), the closure's captures stay live.
    let block = block2::RcBlock::new(move |inst: AudioComponentInstance, status: i32| {
        inst_clone.store(inst.cast(), Ordering::Release);
        status_clone.store(status, Ordering::Release);
        done_clone.store(true, Ordering::Release);
    });
    unsafe {
        AudioComponentInstantiate(
            component,
            AudioComponentInstantiationOptions::empty(),
            &block,
        );
    }

    // Pump the main runloop in 50ms slices until the completion
    // block writes done=true. The runloop dispatches the block to
    // the main queue; without pumping, it'd never fire.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done.load(Ordering::Acquire) {
        if Instant::now() > deadline {
            return Err(Error::LoadFailed {
                path: info_path.to_path_buf(),
                reason: "AudioComponentInstantiate timed out after 10s".into(),
            });
        }
        let mode = unsafe { objc2_core_foundation::kCFRunLoopDefaultMode };
        objc2_core_foundation::CFRunLoop::run_in_mode(mode, 0.05, true);
    }

    let status = status_atom.load(Ordering::Acquire);
    let raw = inst_ptr.load(Ordering::Acquire);
    if status != 0 || raw.is_null() {
        return Err(Error::LoadFailed {
            path: info_path.to_path_buf(),
            reason: format!("AudioComponentInstantiate failed with OSStatus {status}"),
        });
    }
    Ok(raw.cast::<objc2_audio_toolbox::OpaqueAudioComponentInstance>())
}

/// One loaded AU v2 instance. Holds the `AudioComponentInstance`
/// returned by `AudioComponentInstanceNew`. Disposes on `Drop`.
pub struct AuPlugin {
    info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,
    pub(crate) instance: AudioComponentInstance,
    /// Parameter id list, cached at load. AU exposes parameters
    /// by `AudioUnitParameterID` (u32); notwig-rack-core's trait surface
    /// uses zero-based indices. This vec is the index → id map.
    param_ids: Vec<AudioUnitParameterID>,
    /// Stable-address render plumbing — the AU calls our render
    /// callback during `AudioUnitRender` with this Box's pointer as
    /// `inRefCon`. Kept boxed so its address survives moves of
    /// `AuPlugin` itself.
    pub(crate) render_ctx: Option<Box<RenderContext>>,
    /// Storage for the variable-length output `AudioBufferList`
    /// we pass to `AudioUnitRender`. Re-sized on activate based on
    /// channel count.
    pub(crate) output_buffer_storage: Vec<u8>,
    /// Running output sample counter for `AudioTimeStamp.mSampleTime`.
    pub(crate) sample_time: f64,
    /// Sample rate of the active render configuration. AU reports latency in
    /// seconds, so the host-side sample value needs this to convert back.
    pub(crate) sample_rate: f64,
    /// Heap-stable host-transport snapshot whose address is handed
    /// to the AU as `HostCallbackInfo::hostUserData`. `process()`
    /// refreshes it each block; the AU reads it back through the
    /// registered callbacks during `AudioUnitRender`.
    pub(crate) host_transport: Option<Box<HostTransport>>,
    /// Bus channel counts as set during activate (effects need
    /// matching input/output; instruments may have 0 input).
    pub(crate) input_channels: u32,
    pub(crate) output_channels: u32,
    /// Live editor state, populated by `PluginEditor::open`. The
    /// view-factory object (`AUCocoaUIBase`) and the resulting
    /// `NSView` are kept alive here; dropping the plugin tears them
    /// down via objc2's automatic retain/release.
    pub(crate) editor: AuEditorState,
}

// SAFETY: AU v2 instances are not movable across audio rendering
// threads while active, but we serialize all access through
// `&mut self` and never share the pointer.
unsafe impl Send for AuPlugin {}

impl AuPlugin {
    pub(crate) fn load_from(info: &PluginInfo) -> Result<Self> {
        let mut desc = parse_unique_id(&info.unique_id).ok_or_else(|| Error::LoadFailed {
            path: info.path.clone(),
            reason: format!("could not parse AU unique_id {:?}", info.unique_id),
        })?;
        let component = unsafe {
            AudioComponentFindNext(ptr::null_mut(), ptr::NonNull::new_unchecked(&raw mut desc))
        };
        if component.is_null() {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: format!("no AU matching {}", info.unique_id),
            });
        }

        // Re-query the component's actual flags so we can detect
        // RequiresAsyncInstantiation — `desc` only carries our scan
        // mask, not the component's own.
        let mut comp_desc = AudioComponentDescription {
            componentType: 0,
            componentSubType: 0,
            componentManufacturer: 0,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        let _ = unsafe {
            objc2_audio_toolbox::AudioComponentGetDescription(
                component,
                ptr::NonNull::new_unchecked(&raw mut comp_desc),
            )
        };
        let flags = AudioComponentFlags::from_bits_retain(comp_desc.componentFlags);
        let needs_async = flags.contains(AudioComponentFlags::RequiresAsyncInstantiation);

        let instance = if needs_async {
            instantiate_async(component, &info.path)?
        } else {
            instantiate_sync(component, &info.path)?
        };
        let param_ids = unsafe { fetch_parameter_ids(instance) };
        let has_editor = unsafe { has_cocoa_ui(instance) };
        let mut updated = info.clone();
        updated.has_editor = has_editor;
        Ok(Self {
            info: updated,
            layouts: vec![BusLayout::stereo()],
            active_layout: None,
            instance,
            param_ids,
            render_ctx: None,
            output_buffer_storage: Vec::new(),
            sample_time: 0.0,
            sample_rate: 0.0,
            host_transport: None,
            input_channels: 0,
            output_channels: 0,
            editor: AuEditorState::default(),
        })
    }
}

impl Drop for AuPlugin {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            // Tear down the Cocoa view while the AudioUnit is still alive;
            // disposing the instance first leaves the view's callbacks dangling.
            notwig_rack_core::editor::PluginEditor::close(self);
            self.deactivate();
            unsafe { AudioComponentInstanceDispose(self.instance) };
        }
    }
}

impl PluginCore for AuPlugin {
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
        self.param_ids.len()
    }

    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        let id = *self
            .param_ids
            .get(index)
            .ok_or(Error::InvalidParameter(index))?;
        let info = unsafe { fetch_parameter_info(self.instance, id)? };
        Ok(au_parameter_to_rack(id, &info))
    }

    fn parameter_value(&self, index: usize) -> Result<f64> {
        let id = *self
            .param_ids
            .get(index)
            .ok_or(Error::InvalidParameter(index))?;
        let mut value: f32 = 0.0;
        let status = unsafe {
            AudioUnitGetParameter(
                self.instance,
                id,
                kAudioUnitScope_Global,
                0,
                ptr::NonNull::new_unchecked(&raw mut value),
            )
        };
        if status != 0 {
            return Err(Error::Other(format!(
                "AudioUnitGetParameter failed: OSStatus {status}"
            )));
        }
        Ok(f64::from(value))
    }

    fn parameter_value_string(&self, _index: usize, _value: f64) -> Result<String> {
        // AU exposes a parameter-value-to-string property via
        // kAudioUnitProperty_ParameterValueStrings (for indexed
        // params) and AUParameterStringFromValue (for everything
        // else). Neither maps cleanly to the notwig-rack-core surface
        // yet; tracked as a follow-on. For now, fall back to the
        // unit-stripped numeric form so callers get *something*
        // they can display.
        Err(Error::Other(
            "au parameter_value_string not yet wired".into(),
        ))
    }

    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        let id = *self
            .param_ids
            .get(index)
            .ok_or(Error::InvalidParameter(index))?;
        #[allow(clippy::cast_possible_truncation)]
        let v = value as f32;
        let status =
            unsafe { AudioUnitSetParameter(self.instance, id, kAudioUnitScope_Global, 0, v, 0) };
        if status != 0 {
            return Err(Error::Other(format!(
                "AudioUnitSetParameter failed: OSStatus {status}"
            )));
        }
        Ok(())
    }
    fn preset_count(&self) -> usize {
        unsafe { fetch_factory_presets(self.instance) }.map_or(0, |v| v.len())
    }

    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        let presets = unsafe { fetch_factory_presets(self.instance) }
            .ok_or_else(|| Error::Other("kAudioUnitProperty_FactoryPresets failed".into()))?;
        let preset = presets.get(index).ok_or(Error::InvalidParameter(index))?;
        Ok(preset.clone())
    }

    fn load_preset(&mut self, preset_number: i32) -> Result<()> {
        let preset = AUPreset {
            presetNumber: preset_number,
            presetName: ptr::null(),
        };
        let status = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_PresentPreset,
                kAudioUnitScope_Global,
                0,
                (&raw const preset).cast(),
                u32::try_from(std::mem::size_of::<AUPreset>()).unwrap_or(0),
            )
        };
        if status != 0 {
            return Err(Error::Other(format!(
                "AudioUnitSetProperty(PresentPreset) failed: OSStatus {status}"
            )));
        }
        Ok(())
    }
    fn save_state(&self) -> Result<Vec<u8>> {
        use objc2_core_foundation::{
            CFData, CFPropertyList, CFPropertyListCreateData, CFPropertyListFormat, CFRetained,
        };
        let mut class_info: *const CFPropertyList = ptr::null();
        let mut size = u32::try_from(std::mem::size_of::<*const CFPropertyList>()).unwrap_or(0);
        let status = unsafe {
            AudioUnitGetProperty(
                self.instance,
                kAudioUnitProperty_ClassInfo,
                kAudioUnitScope_Global,
                0,
                ptr::NonNull::new_unchecked((&raw mut class_info).cast()),
                ptr::NonNull::new_unchecked(&raw mut size),
            )
        };
        if status != 0 || class_info.is_null() {
            return Err(Error::Other(format!(
                "kAudioUnitProperty_ClassInfo failed: OSStatus {status}"
            )));
        }
        let plist = unsafe {
            CFRetained::<CFPropertyList>::from_raw(ptr::NonNull::new_unchecked(
                class_info.cast_mut(),
            ))
        };
        let mut error: *mut objc2_core_foundation::CFError = ptr::null_mut();
        let data: Option<CFRetained<CFData>> = unsafe {
            CFPropertyListCreateData(
                None,
                Some(&plist),
                CFPropertyListFormat::BinaryFormat_v1_0,
                0,
                &raw mut error,
            )
        };
        let data =
            data.ok_or_else(|| Error::Other("CFPropertyListCreateData returned null".into()))?;
        let len = data.length();
        if len < 0 {
            return Err(Error::Other("CFData length negative".into()));
        }
        #[allow(clippy::cast_sign_loss)]
        let len_usize = len as usize;
        let mut out = vec![0u8; len_usize];
        unsafe {
            data.bytes(
                objc2_core_foundation::CFRange {
                    location: 0,
                    length: len,
                },
                out.as_mut_ptr(),
            );
        }
        Ok(out)
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        use objc2_core_foundation::{
            CFData, CFPropertyList, CFPropertyListCreateWithData, CFPropertyListFormat, CFRetained,
        };
        if bytes.is_empty() {
            return Err(Error::Other("empty AU state".into()));
        }
        #[allow(clippy::cast_possible_wrap)]
        let cf_data: CFRetained<CFData> =
            unsafe { CFData::new(None, bytes.as_ptr(), bytes.len() as isize) }
                .ok_or_else(|| Error::Other("CFData::new returned null".into()))?;
        let mut error: *mut objc2_core_foundation::CFError = ptr::null_mut();
        let mut format = CFPropertyListFormat::BinaryFormat_v1_0;
        let plist: Option<CFRetained<CFPropertyList>> = unsafe {
            CFPropertyListCreateWithData(None, Some(&cf_data), 0, &raw mut format, &raw mut error)
        };
        let plist = plist
            .ok_or_else(|| Error::Other("CFPropertyListCreateWithData returned null".into()))?;
        let plist_ptr: *const CFPropertyList = CFRetained::as_ptr(&plist).as_ptr().cast_const();
        let status = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_ClassInfo,
                kAudioUnitScope_Global,
                0,
                (&raw const plist_ptr).cast(),
                u32::try_from(std::mem::size_of::<*const CFPropertyList>()).unwrap_or(0),
            )
        };
        if status != 0 {
            return Err(Error::Other(format!(
                "AudioUnitSetProperty(ClassInfo) failed: OSStatus {status}"
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
        // Match legacy rack 0.4's sequence: set planar f32 stream
        // format on both scopes (effects need input, instruments
        // don't), set MaximumFramesPerSlice, then
        // AudioUnitInitialize. Sample-rate-dependent parameter
        // ranges (e.g. Apple AUBandpass's Center Frequency cap at
        // Nyquist) only become correct after this point — we
        // refresh the parameter id cache as the last step.
        let channels: u32 = 2;
        #[allow(clippy::cast_possible_truncation)]
        let format = AudioStreamBasicDescription {
            mSampleRate: sample_rate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat
                | kAudioFormatFlagIsPacked
                | kAudioFormatFlagIsNonInterleaved,
            mBytesPerPacket: 4,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4,
            mChannelsPerFrame: channels,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        let _ = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Input,
                0,
                (&raw const format).cast(),
                u32::try_from(std::mem::size_of::<AudioStreamBasicDescription>()).unwrap_or(0),
            )
        };
        let _ = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Output,
                0,
                (&raw const format).cast(),
                u32::try_from(std::mem::size_of::<AudioStreamBasicDescription>()).unwrap_or(0),
            )
        };
        let max_frames = u32::try_from(max_block_size).unwrap_or(u32::MAX);
        let _ = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_MaximumFramesPerSlice,
                kAudioUnitScope_Global,
                0,
                (&raw const max_frames).cast(),
                u32::try_from(std::mem::size_of::<u32>()).unwrap_or(0),
            )
        };
        // Set the render callback before AudioUnitInitialize so
        // the AU sees its input source from frame 0.
        let mut render_ctx = Box::new(RenderContext {
            input_planes: vec![ptr::null(); channels as usize],
            frames: max_block_size,
        });
        let render_ctx_ptr: *mut RenderContext = render_ctx.as_mut();
        let callback_struct = AURenderCallbackStruct {
            inputProc: Some(render_input_callback),
            inputProcRefCon: render_ctx_ptr.cast(),
        };
        // Best-effort: instruments don't accept SetRenderCallback
        // on the input scope. We ignore the OSStatus here for
        // exactly the legacy reasons.
        let _ = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_SetRenderCallback,
                kAudioUnitScope_Input,
                0,
                (&raw const callback_struct).cast(),
                u32::try_from(std::mem::size_of::<AURenderCallbackStruct>()).unwrap_or(0),
            )
        };

        // Host transport callbacks: the AU queries these during
        // AudioUnitRender for tempo / beat / transport state. The
        // boxed HostTransport's address is handed back as
        // inHostUserData. Best-effort — older AUs ignore the
        // property; we don't treat a failure as fatal.
        let mut host_transport = Box::new(HostTransport::default());
        let host_transport_ptr: *mut HostTransport = host_transport.as_mut();
        let host_callbacks = HostCallbackInfo {
            hostUserData: host_transport_ptr.cast(),
            beatAndTempoProc: Some(host_beat_and_tempo),
            musicalTimeLocationProc: Some(host_musical_time),
            transportStateProc: Some(host_transport_state),
            transportStateProc2: Some(host_transport_state2),
        };
        let _ = unsafe {
            AudioUnitSetProperty(
                self.instance,
                kAudioUnitProperty_HostCallbacks,
                kAudioUnitScope_Global,
                0,
                (&raw const host_callbacks).cast(),
                u32::try_from(std::mem::size_of::<HostCallbackInfo>()).unwrap_or(0),
            )
        };

        let status = unsafe { AudioUnitInitialize(self.instance) };
        if status != 0 {
            return Err(Error::Other(format!(
                "AudioUnitInitialize failed: OSStatus {status}"
            )));
        }

        // Refresh the parameter id cache now that initialize has
        // settled the AU's sample-rate-dependent state.
        self.param_ids = unsafe { fetch_parameter_ids(self.instance) };
        self.render_ctx = Some(render_ctx);
        self.host_transport = Some(host_transport);
        self.output_buffer_storage = alloc_audio_buffer_list(channels as usize);
        self.input_channels = channels;
        self.output_channels = channels;
        self.sample_time = 0.0;
        self.sample_rate = sample_rate;
        self.active_layout = Some(layout);
        Ok(())
    }
    fn deactivate(&mut self) {
        if self.is_active() {
            let _ = unsafe { AudioUnitUninitialize(self.instance) };
        }
        self.render_ctx = None;
        self.active_layout = None;
    }
    fn is_active(&self) -> bool {
        self.active_layout.is_some()
    }

    fn latency_samples(&self) -> u32 {
        let mut latency: f64 = 0.0;
        let mut size = u32::try_from(std::mem::size_of::<f64>()).unwrap_or(0);
        let status = unsafe {
            AudioUnitGetProperty(
                self.instance,
                kAudioUnitProperty_Latency,
                kAudioUnitScope_Global,
                0,
                ptr::NonNull::new_unchecked((&raw mut latency).cast()),
                ptr::NonNull::new_unchecked(&raw mut size),
            )
        };
        // AU's kAudioUnitProperty_Latency is expressed in seconds (the truce
        // shim divides the plugin's sample latency by its render rate), while
        // the host trait uses samples.
        if status == 0 && self.sample_rate > 0.0 {
            (latency.max(0.0) * self.sample_rate).round() as u32
        } else {
            0
        }
    }

    fn editor(&mut self) -> Option<&mut dyn notwig_rack_core::editor::PluginEditor> {
        if self.info.has_editor {
            Some(self)
        } else {
            None
        }
    }
}
