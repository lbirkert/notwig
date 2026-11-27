use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use notwig_core::ParamId;
use notwig_core::dsp::DeviceSpec;

use crate::abi::{
    AbiStatus, DeviceStore, pack_ptr_len, packed_pointer, with_audio_frames, with_audio_frames_ptr,
};
use crate::audio_editor::{
    AudioSubEditorContext, AudioSubEditorExtension, AudioSubEditorFrame, AudioSubEditorResponse,
};
use crate::bottom_panel::{
    BottomPanelContext, BottomPanelExtension, BottomPanelFrame, BottomPanelResponse,
};
use crate::command::{
    CommandContext, CommandDescriptor, CommandEffect, CommandExtension, MAX_COMMAND_EDITS,
};
use crate::context::{DeviceDrawContext, DeviceMeterSnapshot};
use crate::extension_point::ExtensionPoint;
use crate::note::{NOTE_EVENT_BYTES, NoteEvent, decode_note_event};
use crate::registration::{
    DeviceRegistrationDescriptor, ExtensionPointDescriptor, ExtensionRegistrations,
};
use crate::rpc::ExtensionCall;

/// Marker interface for the built-in `notwig.devices` extension point.
pub trait DeviceInterface {}

/// Built-in extension point for device implementations.
pub const EP_NOTWIG_DEVICES: ExtensionPoint<dyn DeviceInterface> =
    ExtensionPoint::new("notwig.devices");

/// A device implemented by a WASM extension.
///
/// The host calls [`spec`](Self::spec) once during registration. Each device
/// instance is created with [`new`](Self::new), then receives prepare,
/// parameter, process, and panel draw calls through the ABI.
///
/// Implement [`draw`](Self::draw) with the guest egui API and the
/// shared components in `notwig_ui`. The extension runtime owns the private
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
    /// [`crate::panel_frame::set_param_normalized`] or
    /// [`crate::panel_frame::set_param_value`].
    ///
    /// # Borrow lifetime
    ///
    /// `ui` and its underlying egui context are owned by the extension runtime
    /// and are only valid for the duration of this call. Do not store `ui` or
    /// any reference derived from it; copy out the values you need into device
    /// state. Panels run on the UI/display path, not the audio path.
    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot);

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
    ///
    /// # Real-time
    ///
    /// This method runs on the audio path and must not allocate, block, perform
    /// I/O, load plugins, or run unbounded work. All setup belongs in
    /// [`new`](Self::new) or [`prepare`](Self::prepare), and any runtime problem
    /// should be surfaced through the returned status rather than by unwinding
    /// across the ABI.
    fn process(&mut self, samples: &mut [f32]);

    /// Returns the reported processing latency in samples.
    fn latency(&self) -> u32 {
        0
    }
}

trait DeviceInstance: Send {
    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot);
    fn prepare(&mut self, sample_rate: u32, channels: u16);
    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool;
    fn note_event(&mut self, event: NoteEvent);
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32;
}

impl<T: DeviceExtension> DeviceInstance for T {
    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        DeviceExtension::draw(self, ui, meter);
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

trait SubEditorInstance: Send {
    fn draw(
        &mut self,
        ui: &mut egui::Ui,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse;
}

impl<T: AudioSubEditorExtension> SubEditorInstance for T {
    fn draw(
        &mut self,
        ui: &mut egui::Ui,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse {
        AudioSubEditorExtension::draw(self, ui, context)
    }
}

trait CommandInstance: Send {
    fn execute(&mut self, context: &CommandContext) -> CommandEffect;
}

trait BottomPanelInstance: Send {
    fn draw(&mut self, ui: &mut egui::Ui, context: &BottomPanelContext) -> BottomPanelResponse;
}

impl<T: BottomPanelExtension> BottomPanelInstance for T {
    fn draw(&mut self, ui: &mut egui::Ui, context: &BottomPanelContext) -> BottomPanelResponse {
        BottomPanelExtension::draw(self, ui, context)
    }
}

impl<T: CommandExtension> CommandInstance for T {
    fn execute(&mut self, context: &CommandContext) -> CommandEffect {
        CommandExtension::execute(self, context)
    }
}

struct RegisteredCommand {
    create: fn() -> Box<dyn CommandInstance>,
    descriptor: CommandDescriptor,
}

fn create_command<C: CommandExtension>() -> Box<dyn CommandInstance> {
    Box::new(C::new())
}

struct RegisteredDevice {
    point: String,
    spec_json: Vec<u8>,
    create: fn() -> Box<dyn DeviceInstance>,
}

struct RegisteredAudioSubEditor {
    create: fn() -> Box<dyn SubEditorInstance>,
    descriptor: crate::audio_editor::AudioSubEditorDescriptor,
}

struct RegisteredBottomPanel {
    create: fn() -> Box<dyn BottomPanelInstance>,
    descriptor: crate::bottom_panel::BottomPanelDescriptor,
}

struct DeviceSlot {
    channels: usize,
    instance: Box<dyn DeviceInstance>,
    panel_context: Option<egui::Context>,
    last_panel_bytes: Vec<u8>,
}

struct SubEditorSlot {
    instance: Box<dyn SubEditorInstance>,
    panel_context: Option<egui::Context>,
    last_frame_bytes: Vec<u8>,
}

struct BottomPanelSlot {
    instance: Box<dyn BottomPanelInstance>,
    panel_context: Option<egui::Context>,
    last_frame_bytes: Vec<u8>,
}

fn create_instance<D: DeviceExtension>() -> Box<dyn DeviceInstance> {
    Box::new(D::new())
}

fn create_sub_editor<T: AudioSubEditorExtension>() -> Box<dyn SubEditorInstance> {
    Box::new(T::new())
}

fn create_bottom_panel<T: BottomPanelExtension>() -> Box<dyn BottomPanelInstance> {
    Box::new(T::new())
}

/// Extension runtime shared by the generated ABI exports.
pub struct Extension {
    extension_points: BTreeSet<String>,
    typed_extensions: BTreeMap<String, Vec<Box<dyn ExtensionCall>>>,
    devices: BTreeMap<String, RegisteredDevice>,
    audio_sub_editors: BTreeMap<String, RegisteredAudioSubEditor>,
    bottom_panels: BTreeMap<String, RegisteredBottomPanel>,
    commands: BTreeMap<String, RegisteredCommand>,
    last_command_bytes: Mutex<Vec<u8>>,
    last_registration_bytes: Mutex<Vec<u8>>,
    instances: DeviceStore<DeviceSlot>,
    sub_editor_instances: DeviceStore<SubEditorSlot>,
    bottom_panel_instances: DeviceStore<BottomPanelSlot>,
}

impl Extension {
    /// Creates an empty extension runtime.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            extension_points: BTreeSet::new(),
            typed_extensions: BTreeMap::new(),
            devices: BTreeMap::new(),
            audio_sub_editors: BTreeMap::new(),
            bottom_panels: BTreeMap::new(),
            commands: BTreeMap::new(),
            last_command_bytes: Mutex::new(Vec::new()),
            last_registration_bytes: Mutex::new(Vec::new()),
            instances: DeviceStore::new(),
            sub_editor_instances: DeviceStore::new(),
            bottom_panel_instances: DeviceStore::new(),
        }
    }

    /// Registers one callable typed extension instance on `point`.
    pub fn register_extension<I: ?Sized, T>(&mut self, point: ExtensionPoint<I>, instance: T)
    where
        T: ExtensionCall + 'static,
    {
        self.register_extension_call(point.id(), Box::new(instance));
    }

    /// Registers one callable typed extension instance in the guest runtime.
    #[doc(hidden)]
    pub fn register_extension_call(&mut self, point_id: &str, extension: Box<dyn ExtensionCall>) {
        self.typed_extensions
            .entry(point_id.to_owned())
            .or_default()
            .push(extension);
    }

    /// Returns registered typed extensions for `point_id`.
    #[must_use]
    pub fn extension_calls(&self, point_id: &str) -> &[Box<dyn ExtensionCall>] {
        self.typed_extensions
            .get(point_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Declares one extension point.
    ///
    /// Point declarations belong to the preload phase, before contributors can
    /// register implementations.
    pub fn register_extension_point<T: ?Sized>(&mut self, point: ExtensionPoint<T>) {
        self.extension_points.insert(point.id().to_owned());
    }

    /// Returns the declared extension point ids in stable order.
    pub fn extension_points(&self) -> impl Iterator<Item = &str> {
        self.extension_points.iter().map(String::as_str)
    }

    /// Registers an implementation factory on a typed extension point.
    pub fn register_extension_factory<T: ?Sized>(
        &mut self,
        point: impl crate::lifecycle::ExtensionPointFor<T>,
    ) {
        point.register(self);
    }

    pub(crate) fn register_device_at<D: DeviceExtension>(&mut self, point: &str) {
        let spec = D::spec();
        let type_id = spec.type_id.clone();
        let spec_json = serde_json::to_vec(&spec).unwrap_or_default();
        self.devices.insert(
            type_id,
            RegisteredDevice {
                point: point.to_owned(),
                spec_json,
                create: create_instance::<D>,
            },
        );
    }

    /// Registers an audio sub-editor renderer with runtime metadata.
    pub(crate) fn register_audio_sub_editor_with_descriptor<T: AudioSubEditorExtension>(
        &mut self,
        descriptor: crate::audio_editor::AudioSubEditorDescriptor,
    ) {
        self.audio_sub_editors.insert(
            descriptor.id.clone(),
            RegisteredAudioSubEditor {
                create: create_sub_editor::<T>,
                descriptor,
            },
        );
    }

    /// Registers a bottom panel with runtime metadata.
    pub(crate) fn register_bottom_panel_with_descriptor<T: BottomPanelExtension>(
        &mut self,
        descriptor: crate::bottom_panel::BottomPanelDescriptor,
    ) {
        self.bottom_panels.insert(
            descriptor.id.clone(),
            RegisteredBottomPanel {
                create: create_bottom_panel::<T>,
                descriptor,
            },
        );
    }

    /// Registers a command implementation with runtime metadata.
    pub(crate) fn register_command_with_descriptor<C: CommandExtension>(
        &mut self,
        descriptor: CommandDescriptor,
    ) {
        self.commands.insert(
            descriptor.id.clone(),
            RegisteredCommand {
                create: create_command::<C>,
                descriptor,
            },
        );
    }

    /// Returns the bounded runtime registration manifest.
    #[must_use]
    pub fn registrations(&self) -> ExtensionRegistrations {
        ExtensionRegistrations {
            version: crate::registration::REGISTRATION_MANIFEST_VERSION,
            points: self
                .extension_points
                .iter()
                .map(|id| ExtensionPointDescriptor { id: id.clone() })
                .collect(),
            devices: self
                .devices
                .iter()
                .map(|(type_id, device)| DeviceRegistrationDescriptor {
                    point: device.point.clone(),
                    type_id: type_id.clone(),
                })
                .collect(),
            commands: self
                .commands
                .values()
                .map(|command| command.descriptor.clone())
                .collect(),
            audio_sub_editors: self
                .audio_sub_editors
                .values()
                .map(|editor| editor.descriptor.clone())
                .collect(),
            bottom_panels: self
                .bottom_panels
                .values()
                .map(|panel| panel.descriptor.clone())
                .collect(),
        }
    }

    /// Serializes the registration manifest into the runtime result buffer.
    ///
    /// Returns `None` when serialization fails.
    #[must_use]
    pub fn registrations_result(&self) -> Option<(usize, usize)> {
        let bytes = serde_json::to_vec(&self.registrations()).ok()?;
        if bytes.len() > crate::abi::MAX_OUTPUT_BYTES {
            return None;
        }
        let mut buffer = self.last_registration_bytes.lock().ok()?;
        *buffer = bytes;
        Some((buffer.as_ptr() as usize, buffer.len()))
    }

    /// Returns a packed pointer to the serialized registration manifest.
    #[must_use]
    pub fn registrations_pointer(&self) -> i64 {
        self.registrations_result().map_or(0, |(ptr, len)| {
            let Ok(ptr) = u32::try_from(ptr) else {
                return 0;
            };
            let Ok(len) = u32::try_from(len) else {
                return 0;
            };
            i64::try_from(pack_ptr_len(ptr, len)).unwrap_or(0)
        })
    }

    /// Executes a command with a postcard-encoded context.
    ///
    /// Returns `None` for an unknown command, malformed context, or an effect
    /// that exceeds the host edit budget. The returned bytes contain a
    /// postcard-encoded [`CommandEffect`].
    #[must_use]
    pub fn execute_command_bytes(&self, id: &str, context_bytes: &[u8]) -> Option<Vec<u8>> {
        let command = self.commands.get(id)?;
        let context = postcard::from_bytes::<CommandContext>(context_bytes).ok()?;
        let mut instance = (command.create)();
        let effect = instance.execute(&context);
        match &effect {
            CommandEffect::Apply { edits, .. } if edits.len() > MAX_COMMAND_EDITS => return None,
            CommandEffect::Error { message } if message.len() > 4096 => return None,
            CommandEffect::None | CommandEffect::Apply { .. } | CommandEffect::Error { .. } => {}
        }
        postcard::to_allocvec(&effect).ok()
    }

    /// Executes a command and retains the serialized result in the runtime.
    ///
    /// The returned pointer stays valid until the next command execution. The
    /// host must copy the bytes before invoking another command.
    #[must_use]
    pub fn execute_command_result(&self, id: &str, context_bytes: &[u8]) -> Option<(usize, usize)> {
        let bytes = self.execute_command_bytes(id, context_bytes)?;
        let mut buffer = self.last_command_bytes.lock().ok()?;
        *buffer = bytes;
        Some((buffer.as_ptr() as usize, buffer.len()))
    }

    /// Executes a command and returns a packed pointer/len result for the WASM
    /// ABI.
    #[must_use]
    pub fn execute_command_pointer(&self, id: &str, context_bytes: &[u8]) -> i64 {
        self.execute_command_result(id, context_bytes)
            .map_or(0, |(ptr, len)| {
                let Ok(ptr) = u32::try_from(ptr) else {
                    return 0;
                };
                let Ok(len) = u32::try_from(len) else {
                    return 0;
                };
                i64::try_from(pack_ptr_len(ptr, len)).unwrap_or(0)
            })
    }

    /// Returns a packed pointer to the serialized spec for `type_id`.
    ///
    /// Returns `0` when the type is unknown or the pointer/length does not fit
    /// the packed 32-bit fields. The pointed-to bytes stay valid until that
    /// device type is registered again or the [`Extension`] is dropped; the host
    /// copies the spec immediately after this call.
    #[must_use]
    pub fn describe_pointer(&self, type_id: &str) -> i64 {
        self.describe_bytes(type_id).map_or(0, packed_pointer)
    }

    /// Returns the serialized spec bytes for `type_id`.
    ///
    /// The returned slice borrows this runtime and is valid until the same type
    /// is registered again or the extension is dropped. Returns `None` for an
    /// unknown type.
    #[must_use]
    pub fn describe_bytes(&self, type_id: &str) -> Option<&[u8]> {
        self.devices
            .get(type_id)
            .map(|device| device.spec_json.as_slice())
    }

    /// Creates a device instance and returns its nonzero handle, or `-1` when
    /// `type_id` is unknown.
    ///
    /// This locks and allocates and must not be called from the audio callback.
    pub fn create_device(&self, type_id: &str) -> i32 {
        let Some(device) = self.devices.get(type_id) else {
            return -1;
        };
        let instance = (device.create)();
        self.instances.create(DeviceSlot {
            channels: 2,
            instance,
            panel_context: None,
            last_panel_bytes: Vec::new(),
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
    ///
    /// `audio_ptr` is a guest linear-memory offset to `frames * channels`
    /// interleaved samples. `events` is a sequence of fixed-size
    /// [`NOTE_EVENT_BYTES`] records. Returns [`AbiStatus::UnknownDevice`] for a
    /// missing handle or [`AbiStatus::InvalidArgument`] for an invalid buffer.
    ///
    /// # Real-time
    ///
    /// This is on the audio path and performs no allocation or I/O of its own
    /// beyond the guest device implementation, which must also obey the
    /// real-time rules of [`DeviceExtension::process`].
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
    ///
    /// # Safety
    ///
    /// `audio_ptr` must be valid for exclusive access to `frames * channels`
    /// `f32` samples, 4-byte aligned, and non-aliasing for the duration of the
    /// call. See [`crate::abi::with_audio_frames_ptr`].
    ///
    /// # Real-time
    ///
    /// Runs on the audio path and follows the same no-allocate, no-block,
    /// no-I/O rule as [`process`](Self::process).
    #[allow(unsafe_code)]
    pub unsafe fn process_ptr(
        &self,
        handle: i32,
        audio_ptr: *mut f32,
        frames: usize,
        events: &[u8],
    ) -> Result<(), AbiStatus> {
        let processed = self.instances.with(handle, |slot| {
            deliver_events(slot, events);
            let channels = slot.channels.max(1);
            unsafe {
                with_audio_frames_ptr(audio_ptr, frames, channels, |samples| {
                    slot.instance.process(samples);
                })
            }
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

    /// Creates an audio sub-editor instance and returns its nonzero handle, or
    /// `-1` when `renderer` is unknown.
    pub fn create_audio_sub_editor(&self, renderer: &str) -> i32 {
        let Some(editor) = self.audio_sub_editors.get(renderer) else {
            return -1;
        };
        let instance = (editor.create)();
        self.sub_editor_instances.create(SubEditorSlot {
            instance,
            panel_context: None,
            last_frame_bytes: Vec::new(),
        })
    }

    /// Destroys an audio sub-editor instance.
    pub fn destroy_audio_sub_editor(&self, handle: i32) {
        let _ = self.sub_editor_instances.remove(handle);
    }

    /// Creates a bottom panel instance, or `-1` when `panel_id` is unknown.
    pub fn create_bottom_panel(&self, panel_id: &str) -> i32 {
        let Some(panel) = self.bottom_panels.get(panel_id) else {
            return -1;
        };
        let instance = (panel.create)();
        self.bottom_panel_instances.create(BottomPanelSlot {
            instance,
            panel_context: None,
            last_frame_bytes: Vec::new(),
        })
    }

    /// Destroys a bottom panel instance.
    pub fn destroy_bottom_panel(&self, handle: i32) {
        let _ = self.bottom_panel_instances.remove(handle);
    }

    /// Draws a bottom panel and returns a packed pointer to its frame.
    #[must_use]
    pub fn draw_bottom_panel_pointer(&self, handle: i32, context_bytes: &[u8]) -> i64 {
        let Some((ptr, len)) = self.draw_bottom_panel_bytes(handle, context_bytes) else {
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

    /// Draws a bottom panel and returns its validated frame.
    #[must_use]
    pub fn draw_bottom_panel_frame(
        &self,
        handle: i32,
        context_bytes: &[u8],
    ) -> Option<BottomPanelFrame> {
        let context = postcard::from_bytes::<BottomPanelContext>(context_bytes).ok()?;
        context.input.validate().ok()?;
        self.bottom_panel_instances
            .with(handle, move |slot| {
                let BottomPanelSlot {
                    instance,
                    panel_context,
                    ..
                } = slot;
                let panel_context = panel_context.get_or_insert_with(egui::Context::default);
                let mut response = BottomPanelResponse::none();
                let panel = crate::panel_frame::run_frame(
                    panel_context,
                    context.input.clone(),
                    &context.theme,
                    |ui| {
                        response = instance.draw(ui, &context);
                    },
                );
                let frame = BottomPanelFrame {
                    panel,
                    effect: response.effect,
                };
                frame.validate().ok()?;
                Some(frame)
            })
            .ok()
            .flatten()
    }

    /// Draws a bottom panel and returns a pointer/length pair to its frame.
    #[must_use]
    pub fn draw_bottom_panel_bytes(
        &self,
        handle: i32,
        context_bytes: &[u8],
    ) -> Option<(*const u8, usize)> {
        let frame = self.draw_bottom_panel_frame(handle, context_bytes)?;
        let bytes = postcard::to_allocvec(&frame).ok()?;
        self.bottom_panel_instances
            .with(handle, move |slot| {
                slot.last_frame_bytes = bytes;
                (slot.last_frame_bytes.as_ptr(), slot.last_frame_bytes.len())
            })
            .ok()
    }

    /// Draws an audio sub-editor and returns a packed pointer to the serialized
    /// [`AudioSubEditorFrame`].
    #[must_use]
    pub fn draw_audio_sub_editor_pointer(&self, handle: i32, context_bytes: &[u8]) -> i64 {
        let Some((ptr, len)) = self.draw_audio_sub_editor_bytes(handle, context_bytes) else {
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

    /// Draws an audio sub-editor and returns its validated frame.
    #[must_use]
    pub fn draw_audio_sub_editor_frame(
        &self,
        handle: i32,
        context_bytes: &[u8],
    ) -> Option<AudioSubEditorFrame> {
        let context = postcard::from_bytes::<AudioSubEditorContext>(context_bytes).ok()?;
        context.input.validate().ok()?;
        self.sub_editor_instances
            .with(handle, move |slot| {
                let SubEditorSlot {
                    instance,
                    panel_context,
                    ..
                } = slot;
                let panel_context = panel_context.get_or_insert_with(egui::Context::default);
                let mut response = AudioSubEditorResponse::none();
                let panel = crate::panel_frame::run_frame(
                    panel_context,
                    context.input.clone(),
                    &context.theme,
                    |ui| {
                        response = instance.draw(ui, &context);
                    },
                );
                let frame = AudioSubEditorFrame {
                    panel,
                    effect: response.effect,
                    clipboard: response.clipboard,
                };
                frame.validate().ok()?;
                Some(frame)
            })
            .ok()
            .flatten()
    }

    /// Draws an audio sub-editor and returns a pointer/length pair to the
    /// serialized [`AudioSubEditorFrame`].
    #[must_use]
    pub fn draw_audio_sub_editor_bytes(
        &self,
        handle: i32,
        context_bytes: &[u8],
    ) -> Option<(*const u8, usize)> {
        let frame = self.draw_audio_sub_editor_frame(handle, context_bytes)?;
        let bytes = postcard::to_allocvec(&frame).ok()?;
        self.sub_editor_instances
            .with(handle, move |slot| {
                slot.last_frame_bytes = bytes;
                (slot.last_frame_bytes.as_ptr(), slot.last_frame_bytes.len())
            })
            .ok()
    }

    /// Draws a device panel and returns a packed pointer to the serialized
    /// [`crate::panel_frame::PanelFrame`].
    #[must_use]
    pub fn draw_pointer(&self, handle: i32, context_bytes: &[u8]) -> i64 {
        let Some((ptr, len)) = self.draw_bytes(handle, context_bytes) else {
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
    /// [`crate::panel_frame::PanelFrame`].
    ///
    /// The pointer stays valid until the next draw on the same handle, after
    /// which the backing bytes may be replaced. The host must copy or consume
    /// the frame before drawing that handle again. Returns `None` for an
    /// unknown handle, an invalid context or input snapshot, or a frame that
    /// fails validation.
    #[must_use]
    pub fn draw_bytes(&self, handle: i32, context_bytes: &[u8]) -> Option<(*const u8, usize)> {
        let context = postcard::from_bytes::<DeviceDrawContext>(context_bytes).ok()?;
        context.input.validate().ok()?;
        let DeviceDrawContext {
            input,
            theme,
            meter,
            ..
        } = context;
        self.instances
            .with(handle, move |slot| {
                let DeviceSlot {
                    instance,
                    panel_context,
                    last_panel_bytes,
                    ..
                } = slot;
                let context = panel_context.get_or_insert_with(egui::Context::default);
                let frame = crate::panel_frame::run_frame(context, input, &theme, |ui| {
                    instance.draw(ui, meter);
                });
                if frame.validate().is_err() {
                    return None;
                }
                *last_panel_bytes = postcard::to_allocvec(&frame).ok()?;
                Some((last_panel_bytes.as_ptr(), last_panel_bytes.len()))
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
