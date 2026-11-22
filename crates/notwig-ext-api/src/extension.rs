use std::collections::BTreeMap;

use notwig_core::dsp::DeviceSpec;
use notwig_core::ParamId;

use crate::abi::{
    AbiStatus, DeviceStore, pack_ptr_len, packed_pointer, with_audio_frames, with_audio_frames_ptr,
};
use crate::context::{DeviceDrawContext, DeviceMeterSnapshot};
use crate::note::{NOTE_EVENT_BYTES, NoteEvent, decode_note_event};

/// A device implemented by a WASM extension.
///
/// The host calls [`spec`](Self::spec) once during registration. Each device
/// instance is created with [`new`](Self::new), then receives prepare,
/// parameter, process, and panel draw calls through the ABI.
///
/// Implement [`draw_egui`](Self::draw_egui) with the guest egui API and the
/// shared components in [`notwig_ui`]. The extension runtime owns the private
/// egui context, so implementations only need to keep their own view state on
/// the device.
pub trait DeviceExtension: Send + Sized + 'static {
    /// Creates a device instance.
    fn new() -> Self;

    /// Describes the device and its parameters.
    fn spec() -> DeviceSpec;

    /// Draws the device panel with the real egui API inside the guest.
    ///
    /// `meter` carries the latest engine-measured input/output peaks.
    /// Parameter changes must go through
    /// [`crate::egui_panel::set_param_normalized`] or
    /// [`crate::egui_panel::set_param_value`].
    fn draw_egui(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot);

    /// Prepares the processor for a sample rate and channel count.
    fn prepare(&mut self, _sample_rate: u32, _channels: u16) {}

    /// Applies a parameter value from the host.
    ///
    /// Returns `true` when the parameter was recognized.
    fn set_param(&mut self, _param_id: ParamId, _value: f64) -> bool {
        false
    }

    /// Handles one note event before [`process`](Self::process) runs.
    ///
    /// Events are delivered in block order and `sample_offset` is relative to
    /// the start of the upcoming block. Effect devices can ignore this hook.
    fn note_event(&mut self, _event: NoteEvent) {}

    /// Processes interleaved `f32` samples in place.
    fn process(&mut self, samples: &mut [f32]);

    /// Returns the reported processing latency in samples.
    fn latency(&self) -> u32 {
        0
    }
}

trait DeviceInstance: Send {
    fn draw_egui(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot);
    fn prepare(&mut self, sample_rate: u32, channels: u16);
    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool;
    fn note_event(&mut self, event: NoteEvent);
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32;
}

impl<T: DeviceExtension> DeviceInstance for T {
    fn draw_egui(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        DeviceExtension::draw_egui(self, ui, meter);
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        DeviceExtension::prepare(self, sample_rate, channels);
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        DeviceExtension::set_param(self, param_id, value)
    }

    fn note_event(&mut self, event: NoteEvent) {
        DeviceExtension::note_event(self, event);
    }

    fn process(&mut self, samples: &mut [f32]) {
        DeviceExtension::process(self, samples);
    }

    fn latency(&self) -> u32 {
        DeviceExtension::latency(self)
    }
}

struct RegisteredDevice {
    spec_json: Vec<u8>,
    create: fn() -> Box<dyn DeviceInstance>,
}

struct DeviceSlot {
    channels: usize,
    instance: Box<dyn DeviceInstance>,
    egui_context: Option<egui::Context>,
    last_egui_bytes: Vec<u8>,
}

fn create_instance<D: DeviceExtension>() -> Box<dyn DeviceInstance> {
    Box::new(D::new())
}

/// Extension runtime shared by the generated ABI exports.
pub struct Extension {
    devices: BTreeMap<String, RegisteredDevice>,
    instances: DeviceStore<DeviceSlot>,
}

impl Extension {
    /// Creates an empty extension runtime.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            devices: BTreeMap::new(),
            instances: DeviceStore::new(),
        }
    }

    /// Registers one device implementation.
    pub fn register_device<D: DeviceExtension>(&mut self) {
        let spec = D::spec();
        let type_id = spec.type_id.clone();
        let spec_json = serde_json::to_vec(&spec).unwrap_or_default();
        self.devices.insert(
            type_id,
            RegisteredDevice {
                spec_json,
                create: create_instance::<D>,
            },
        );
    }

    /// Returns a packed pointer to the serialized spec for `type_id`.
    #[must_use]
    pub fn describe_pointer(&self, type_id: &str) -> i64 {
        self.describe_bytes(type_id).map_or(0, packed_pointer)
    }

    /// Returns the serialized spec bytes for `type_id`.
    #[must_use]
    pub fn describe_bytes(&self, type_id: &str) -> Option<&[u8]> {
        self.devices
            .get(type_id)
            .map(|device| device.spec_json.as_slice())
    }

    /// Creates a device instance and returns its handle.
    pub fn create_device(&self, type_id: &str) -> i32 {
        let Some(device) = self.devices.get(type_id) else {
            return -1;
        };
        let instance = (device.create)();
        self.instances.create(DeviceSlot {
            channels: 2,
            instance,
            egui_context: None,
            last_egui_bytes: Vec::new(),
        })
    }

    /// Destroys a device instance.
    pub fn destroy_device(&self, handle: i32) {
        let _ = self.instances.remove(handle);
    }

    /// Prepares a device instance.
    pub fn prepare(&self, handle: i32, sample_rate: u32, channels: u16) -> Result<(), AbiStatus> {
        self.instances.with(handle, |slot| {
            slot.channels = usize::from(channels.max(1));
            slot.instance.prepare(sample_rate, channels);
        })
    }

    /// Applies a parameter value to a device instance.
    pub fn set_param(&self, handle: i32, param_id: ParamId, value: f64) -> Result<bool, AbiStatus> {
        self.instances
            .with(handle, |slot| slot.instance.set_param(param_id, value))
    }

    /// Processes one block of interleaved audio after delivering note events.
    pub fn process(
        &self,
        handle: i32,
        audio_ptr: i32,
        frames: usize,
        events: &[u8],
    ) -> Result<(), AbiStatus> {
        let processed = self.instances.with(handle, |slot| {
            deliver_events(slot, events);
            let channels = slot.channels.max(1);
            with_audio_frames(audio_ptr, frames, channels, |samples| {
                slot.instance.process(samples);
            })
        })?;
        processed.ok_or(AbiStatus::InvalidArgument)
    }

    /// Processes one block from a native host pointer.
    ///
    /// The native shared-library ABI passes ordinary process pointers, so the
    /// host can process its audio buffer in place without copying.
    pub fn process_ptr(
        &self,
        handle: i32,
        audio_ptr: *mut f32,
        frames: usize,
        events: &[u8],
    ) -> Result<(), AbiStatus> {
        let processed = self.instances.with(handle, |slot| {
            deliver_events(slot, events);
            let channels = slot.channels.max(1);
            with_audio_frames_ptr(audio_ptr, frames, channels, |samples| {
                slot.instance.process(samples);
            })
        })?;
        processed.ok_or(AbiStatus::InvalidArgument)
    }

    /// Returns the reported device latency.
    #[must_use]
    pub fn latency(&self, handle: i32) -> u32 {
        self.instances
            .with(handle, |slot| slot.instance.latency())
            .unwrap_or(0)
    }

    /// Draws an egui panel and returns a packed pointer to the serialized
    /// [`crate::egui_panel::EguiFrame`].
    #[must_use]
    pub fn draw_egui_pointer(&self, handle: i32, context_bytes: &[u8]) -> i64 {
        let Some((ptr, len)) = self.draw_egui_bytes(handle, context_bytes) else {
            return 0;
        };
        let Ok(ptr) = u32::try_from(ptr as usize) else {
            return 0;
        };
        let Ok(len) = u32::try_from(len) else {
            return 0;
        };
        i64::try_from(pack_ptr_len(ptr, len)).unwrap_or(0)
    }

    /// Draws an egui panel and returns a pointer/length pair to the serialized
    /// [`crate::egui_panel::EguiFrame`].
    ///
    /// The pointer stays valid until the next draw on the same handle.
    #[must_use]
    pub fn draw_egui_bytes(&self, handle: i32, context_bytes: &[u8]) -> Option<(*const u8, usize)> {
        let context = postcard::from_bytes::<DeviceDrawContext>(context_bytes).ok()?;
        context.egui.validate().ok()?;
        let DeviceDrawContext {
            egui: input,
            theme,
            meter,
            ..
        } = context;
        self.instances
            .with(handle, move |slot| {
                let DeviceSlot {
                    instance,
                    egui_context,
                    last_egui_bytes,
                    ..
                } = slot;
                let context = egui_context.get_or_insert_with(egui::Context::default);
                let frame = crate::egui_panel::run_frame(context, input, &theme, |ui| {
                    instance.draw_egui(ui, meter);
                });
                if frame.validate().is_err() {
                    return None;
                }
                *last_egui_bytes = postcard::to_allocvec(&frame).ok()?;
                Some((last_egui_bytes.as_ptr(), last_egui_bytes.len()))
            })
            .ok()
            .flatten()
    }
}

fn deliver_events(slot: &mut DeviceSlot, events: &[u8]) {
    for bytes in events.chunks_exact(NOTE_EVENT_BYTES) {
        if let Some(event) = decode_note_event(bytes) {
            slot.instance.note_event(event);
        }
    }
}

impl Default for Extension {
    fn default() -> Self {
        Self::new()
    }
}
