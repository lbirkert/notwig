use std::collections::BTreeMap;

use notwig_core::ParamId;
use notwig_dsp::DeviceSpec;

use crate::abi::{
    AbiStatus, DeviceStore, pack_ptr_len, packed_pointer, with_audio_frames, with_audio_frames_ptr,
};
use crate::context::DeviceDrawContext;
use crate::geometry::{Point, Rect, Size};
use crate::note::{NOTE_EVENT_BYTES, NoteEvent, decode_note_event};
use crate::ui::{RecordUi, Style, UiContext, UiMemory};

/// A device implemented by a WASM extension.
///
/// The host calls [`spec`](Self::spec) once during registration. Each device
/// instance is created with [`new`](Self::new), then receives prepare,
/// parameter, process, and panel draw calls through the ABI.
///
/// Implement [`draw`](Self::draw) with [`UiContext`] and the shared widgets in
/// [`crate::ui`]. The extension runtime owns retained interaction memory, so
/// implementations only need to keep their own view state on the device.
pub trait DeviceExtension: Send + Sized + 'static {
    /// Creates a device instance.
    fn new() -> Self;

    /// Describes the device and its parameters.
    fn spec() -> DeviceSpec;

    /// Draws the device panel for the current frame.
    ///
    /// Use [`UiContext::interact`] or shared widgets such as
    /// [`crate::ui::button`] and [`crate::ui::knob`] for interaction. Parameter
    /// changes must go through [`UiContext::set_param_normalized`] or
    /// [`UiContext::set_param_value`] so the host can validate them.
    fn draw(&mut self, ui: &mut UiContext<'_>);

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
    fn draw(&mut self, ui: &mut UiContext<'_>);
    fn prepare(&mut self, sample_rate: u32, channels: u16);
    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool;
    fn note_event(&mut self, event: NoteEvent);
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32;
}

impl<T: DeviceExtension> DeviceInstance for T {
    fn draw(&mut self, ui: &mut UiContext<'_>) {
        DeviceExtension::draw(self, ui);
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
    ui_memory: UiMemory,
    last_panel_json: Vec<u8>,
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
            ui_memory: UiMemory::new(),
            last_panel_json: Vec::new(),
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

    /// Draws a device panel and returns a packed pointer to the serialized
    /// [`crate::panel::PanelDraw`].
    ///
    /// Returns `0` for an unknown handle, invalid context JSON, or invalid
    /// panel output.
    #[must_use]
    pub fn draw_pointer(&self, handle: i32, context_json: &[u8]) -> i64 {
        let Some((ptr, len)) = self.draw_json(handle, context_json) else {
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

    /// Draws a device panel and returns a pointer/length pair to the serialized
    /// [`crate::panel::PanelDraw`].
    ///
    /// The pointer stays valid until the next draw on the same handle.
    #[must_use]
    pub fn draw_json(&self, handle: i32, context_json: &[u8]) -> Option<(*const u8, usize)> {
        let context = serde_json::from_slice::<DeviceDrawContext>(context_json).ok()?;
        self.instances
            .with(handle, |slot| {
                let target = Rect::from_min_size(
                    Point::new(0.0, 0.0),
                    Size::new(context.width, context.height),
                );
                let style = Style::from_theme(context.theme);
                let mut record = RecordUi::new(&context.input, &mut slot.ui_memory);
                {
                    let mut ui =
                        UiContext::new_with_meter(&mut record, style, target, context.meter);
                    slot.instance.draw(&mut ui);
                }
                let panel = record.into_panel();
                if panel.validate().is_err() {
                    return None;
                }
                slot.last_panel_json = serde_json::to_vec(&panel).unwrap_or_default();
                Some((slot.last_panel_json.as_ptr(), slot.last_panel_json.len()))
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
