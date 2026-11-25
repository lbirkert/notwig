use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::bus::BusLayout;
use notwig_rack_core::editor::PluginEditor;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::{Event, EventList};
use notwig_rack_core::info::{ParameterInfo, PluginInfo, PresetInfo};
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};
use notwig_rack_core::transport::TransportInfo;

use lv2_raw::atom::{LV2Atom, LV2AtomEvent, LV2AtomSequence, LV2AtomSequenceBody};
use lv2_raw::urid::LV2Urid;

use std::ffi::{CStr, CString, c_char, c_void};
use std::mem;

use crate::features::Features;
use crate::midi::{decode_midi_atom, midi_bytes};
use crate::ports::{PortInfo, PortKind, audio_layout, classify_ports};
use crate::scanner::World;
use crate::time_atom::{TIME_POSITION_MAX_BYTES, TimeUrids, write_time_position};
use crate::ui::{Controller, EditorState, UiBundle, discover_ui};

const LV2_ATOM_SEQUENCE_URI: &[u8] = b"http://lv2plug.in/ns/ext/atom#Sequence\0";

/// One loaded LV2 plugin.
///
/// Owns its `World` (lilv data is referenced by pointer from the
/// instance), its features, its per-port metadata, and the per-port
/// scratch buffers connected on every block.
pub struct Lv2Plugin {
    pub(crate) info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,

    /// Held for ownership: the `LilvPlugin` pointer and the instance
    /// both reference world-owned data, so the world must outlive
    /// them. Read directly only at load.
    #[allow(dead_code)]
    world: Box<World>,
    plugin: *const lilv_sys::LilvPlugin,
    instance: *mut lilv_sys::LilvInstance,

    sample_rate: f64,

    ports: Vec<PortInfo>,
    /// Backing storage for control ports. `connect_port` points the
    /// plugin at one f32 per control port; the index here matches
    /// the position in `ports` filtered to controls.
    pub(crate) control_values: Vec<f32>,
    /// Map from `ports` index to the offset in `control_values`.
    pub(crate) control_value_offset: Vec<usize>,
    /// Host-facing control-port parameters (input controls only).
    parameters: Vec<ParameterInfo>,
    /// Offset in `control_values` for the `lv2:reportsLatency` output port.
    latency_offset: Option<usize>,
    /// State loaded before the instance exists; applied on first activate.
    pending_state: Option<Vec<u8>>,

    /// Backing storage for the (single) MIDI input atom-sequence
    /// port, if the plugin has one. Rebuilt each block from the
    /// notwig-rack `EventList`.
    midi_in_buf: Vec<u8>,
    /// Backing storage for the MIDI output atom-sequence port. We
    /// connect a chunk-typed buffer so the plugin has somewhere to
    /// write; we don't currently drain it back into the host.
    midi_out_buf: Vec<u8>,

    /// Reused per-block channel-pointer slabs for the connected audio ports.
    /// Only valid for the duration of one `process` call.
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,

    pub(crate) features: Box<Features>,
    midi_event_urid: LV2Urid,
    atom_sequence_urid: LV2Urid,
    time_urids: TimeUrids,

    /// UI metadata captured at load if the plugin advertises a
    /// native UI for this platform. `None` means no editor — the
    /// `PluginEditor` impl will refuse `open`.
    pub(crate) ui_bundle: Option<UiBundle>,
    /// Live editor — `Some` between `open` and `close`.
    pub(crate) editor: Option<EditorState>,
    /// Heap-stable controller passed to the LV2 UI as the opaque
    /// callback context. Built lazily on first `open`. Boxed so its
    /// address survives moves of `Lv2Plugin`.
    pub(crate) controller: Option<Box<Controller>>,
}

// SAFETY: We hand the plugin between the audio and main threads
// behind an `Arc<Mutex<_>>` exactly like every other notwig-rack format.
unsafe impl Send for Lv2Plugin {}

impl Lv2Plugin {
    pub(crate) fn load_from(info: &PluginInfo) -> Result<Self> {
        let world = Box::new(unsafe { World::new() }.ok_or_else(|| Error::LoadFailed {
            path: info.path.clone(),
            reason: "lilv_world_new returned NULL".into(),
        })?);
        unsafe { lilv_sys::lilv_world_load_all(world.ptr) };

        let uri = CString::new(info.unique_id.clone()).map_err(|_| Error::LoadFailed {
            path: info.path.clone(),
            reason: format!("lv2 unique_id contains NUL: {:?}", info.unique_id),
        })?;
        let uri_node = unsafe { lilv_sys::lilv_new_uri(world.ptr, uri.as_ptr()) };
        if uri_node.is_null() {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: format!("lilv_new_uri failed for {:?}", info.unique_id),
            });
        }
        let plugins = unsafe { lilv_sys::lilv_world_get_all_plugins(world.ptr) };
        let plugin = unsafe { lilv_sys::lilv_plugins_get_by_uri(plugins, uri_node) };
        unsafe { lilv_sys::lilv_node_free(uri_node) };
        if plugin.is_null() {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: format!("no LV2 plugin matching URI {:?}", info.unique_id),
            });
        }

        // Walk the ports once, classify each, and stash
        // direction + control defaults.
        let classified = unsafe { classify_ports(world.ptr, plugin) };
        let crate::ports::ClassifiedPorts {
            ports,
            audio_in_count,
            audio_out_count,
            control_value_offset,
            control_values,
            parameters,
            latency_offset,
        } = classified;

        // Pre-map the URIs we'll need at process time.
        let features = Features::new();
        let midi_event_urid = features
            .urid_map
            .map_uri(unsafe { CStr::from_ptr(lilv_sys::LILV_URI_MIDI_EVENT.as_ptr().cast()) });
        let atom_sequence_urid = features
            .urid_map
            .map_uri(unsafe { CStr::from_ptr(LV2_ATOM_SEQUENCE_URI.as_ptr().cast()) });

        // URIDs for the host transport (`time:Position`) atom we
        // inject each block. Mapped once here so process() stays
        // allocation-free.
        let map_uri = |uri: &[u8]| {
            features
                .urid_map
                .map_uri(unsafe { CStr::from_ptr(uri.as_ptr().cast()) })
        };
        let time_urids = TimeUrids {
            position: map_uri(lv2_raw::time::LV2_TIME__POSITION),
            frame: map_uri(lv2_raw::time::LV2_TIME__FRAME),
            speed: map_uri(lv2_raw::time::LV2_TIME__SPEED),
            bar: map_uri(lv2_raw::time::LV2_TIME__BAR),
            bar_beat: map_uri(lv2_raw::time::LV2_TIME__BARBEAT),
            beats_per_bar: map_uri(lv2_raw::time::LV2_TIME__BEATSPERBAR),
            beat_unit: map_uri(lv2_raw::time::LV2_TIME__BEATUNIT),
            bpm: map_uri(lv2_raw::time::LV2_TIME__BEATSPERMINUTE),
            atom_long: map_uri(lv2_raw::atom::LV2_ATOM__LONG),
            atom_float: map_uri(lv2_raw::atom::LV2_ATOM__FLOAT),
            atom_int: map_uri(lv2_raw::atom::LV2_ATOM__INT),
            atom_object: map_uri(lv2_raw::atom::LV2_ATOM__OBJECT),
        };

        let layout = audio_layout(audio_in_count, audio_out_count);

        let ui_bundle = unsafe { discover_ui(world.ptr, plugin) };
        let mut info = info.clone();
        info.has_editor = ui_bundle.is_some();

        Ok(Self {
            info,
            layouts: vec![layout],
            active_layout: None,
            world,
            plugin,
            instance: std::ptr::null_mut(),
            sample_rate: 0.0,
            ports,
            control_values,
            control_value_offset,
            parameters,
            latency_offset,
            pending_state: None,
            midi_in_buf: Vec::new(),
            midi_out_buf: Vec::new(),
            in_ptrs: Vec::new(),
            out_ptrs: Vec::new(),
            features,
            midi_event_urid,
            atom_sequence_urid,
            time_urids,
            ui_bundle,
            editor: None,
            controller: None,
        })
    }

    fn build_midi_sequence(&mut self, events: &EventList, transport: Option<TransportInfo>) {
        let header_size = mem::size_of::<LV2AtomSequence>();
        let cap = self.midi_in_buf.len();
        if cap < header_size {
            return;
        }
        let buf = self.midi_in_buf.as_mut_ptr();

        // Header: capacity-bounded sequence whose `atom.size` only
        // counts the body + events (the LV2 atom convention).
        let body_size = mem::size_of::<LV2AtomSequenceBody>();
        let seq = unsafe { &mut *buf.cast::<LV2AtomSequence>() };
        seq.atom = LV2Atom {
            size: body_size as u32,
            mytype: self.atom_sequence_urid,
        };
        seq.body = LV2AtomSequenceBody { unit: 0, pad: 0 };

        let mut write_off = header_size;

        // Inject a `time:Position` object as the first event (frame
        // 0) so transport-aware plugins see tempo / grid before any
        // MIDI. MIDI events follow, also at frame >= 0.
        if let Some(t) = transport {
            write_off = unsafe { write_time_position(&self.time_urids, buf, write_off, cap, &t) };
        }

        let event_header = mem::size_of::<LV2AtomEvent>();
        for ev in events {
            let Some((status, d1, d2, len)) = midi_bytes(&ev.body) else {
                continue;
            };
            let total = event_header + len;
            let padded = (total + 7) & !7; // 8-byte alignment per atom spec
            if write_off + padded > cap {
                break;
            }
            let event_ptr = unsafe { buf.add(write_off) }.cast::<LV2AtomEvent>();
            unsafe {
                (*event_ptr).time_in_frames = i64::from(ev.sample_offset);
                (*event_ptr).body = LV2Atom {
                    size: len as u32,
                    mytype: self.midi_event_urid,
                };
                let data_ptr = (event_ptr as *mut u8).add(event_header);
                if len >= 1 {
                    *data_ptr = status;
                }
                if len >= 2 {
                    *data_ptr.add(1) = d1;
                }
                if len >= 3 {
                    *data_ptr.add(2) = d2;
                }
            }
            write_off += padded;
        }
        // sequence body size is everything past the atom header
        seq.atom.size = (write_off - mem::size_of::<LV2Atom>()) as u32;
    }

    fn prep_midi_out(&mut self) {
        // Reset to an empty Chunk-typed atom so the plugin has a
        // defined buffer to write into. We don't drain output yet.
        if self.midi_out_buf.len() < mem::size_of::<LV2AtomSequence>() {
            return;
        }
        let buf = self.midi_out_buf.as_mut_ptr();
        let seq = unsafe { &mut *buf.cast::<LV2AtomSequence>() };
        seq.atom = LV2Atom {
            size: mem::size_of::<LV2AtomSequenceBody>() as u32,
            mytype: self.atom_sequence_urid,
        };
        seq.body = LV2AtomSequenceBody { unit: 0, pad: 0 };
    }
}

/// Suppress unused-import warning for `Event`.
#[allow(dead_code)]
fn _event_marker(_: &Event) {}

impl Drop for Lv2Plugin {
    fn drop(&mut self) {
        // Tear down the editor first so the UI's `cleanup` runs
        // while the audio instance is still alive (some UIs talk
        // to the instance via the `instance-access` extension).
        self.close_editor();
        if !self.instance.is_null() {
            unsafe {
                lilv_sys::lilv_instance_deactivate(self.instance);
                lilv_sys::lilv_instance_free(self.instance);
            }
            self.instance = std::ptr::null_mut();
        }
        // `world` and `features` drop in struct order after this.
    }
}

impl Lv2Plugin {
    fn restore_state_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let text = CString::new(bytes)
            .map_err(|_| Error::Other("lv2 state blob contains an embedded NUL".into()))?;
        let state = unsafe {
            lilv_sys::lilv_state_new_from_string(
                self.world.ptr,
                self.features.map_ptr(),
                text.as_ptr(),
            )
        };
        if state.is_null() {
            return Err(Error::Other("lv2 state parse failed".into()));
        }

        let mut ctx = RestorePortValues {
            ports: &self.ports,
            offsets: &self.control_value_offset,
            values: self.control_values.as_mut_ptr(),
            float_type: self.time_urids.atom_float,
        };
        let ctx_ptr = (&mut ctx as *mut RestorePortValues).cast::<c_void>();
        if self.instance.is_null() {
            unsafe {
                lilv_sys::lilv_state_emit_port_values(state, Some(set_port_value), ctx_ptr);
            }
            self.pending_state = Some(bytes.to_vec());
        } else {
            unsafe {
                lilv_sys::lilv_state_restore(
                    state,
                    self.instance,
                    Some(set_port_value),
                    ctx_ptr,
                    0,
                    self.features.feature_ptrs.as_ptr(),
                );
            }
        }
        unsafe { lilv_sys::lilv_state_free(state) };
        Ok(())
    }
}

struct SavePortValues<'a> {
    ports: &'a [PortInfo],
    offsets: &'a [usize],
    values: &'a [f32],
    scratch: f32,
    float_type: u32,
}

unsafe extern "C" fn get_port_value(
    symbol: *const c_char,
    user_data: *mut c_void,
    size: *mut u32,
    value_type: *mut u32,
) -> *const c_void {
    if symbol.is_null() || user_data.is_null() {
        return std::ptr::null();
    }
    let ctx = unsafe { &mut *user_data.cast::<SavePortValues>() };
    let symbol = unsafe { CStr::from_ptr(symbol) }.to_bytes();
    for (index, port) in ctx.ports.iter().enumerate() {
        if port.symbol.as_bytes() != symbol {
            continue;
        }
        let offset = ctx.offsets[index];
        if offset == usize::MAX {
            return std::ptr::null();
        }
        ctx.scratch = ctx.values[offset];
        if !size.is_null() {
            unsafe { *size = std::mem::size_of::<f32>() as u32 };
        }
        if !value_type.is_null() {
            unsafe { *value_type = ctx.float_type };
        }
        return (&ctx.scratch as *const f32).cast::<c_void>();
    }
    std::ptr::null()
}

struct RestorePortValues<'a> {
    ports: &'a [PortInfo],
    offsets: &'a [usize],
    values: *mut f32,
    float_type: u32,
}

unsafe extern "C" fn set_port_value(
    symbol: *const c_char,
    user_data: *mut c_void,
    value: *const c_void,
    size: u32,
    value_type: u32,
) {
    if symbol.is_null() || user_data.is_null() || value.is_null() {
        return;
    }
    let ctx = unsafe { &mut *user_data.cast::<RestorePortValues>() };
    if value_type != ctx.float_type || size != std::mem::size_of::<f32>() as u32 {
        return;
    }
    let value = unsafe { *value.cast::<f32>() };
    let symbol = unsafe { CStr::from_ptr(symbol) }.to_bytes();
    for (index, port) in ctx.ports.iter().enumerate() {
        if port.symbol.as_bytes() != symbol {
            continue;
        }
        let offset = ctx.offsets[index];
        if offset != usize::MAX {
            unsafe { *ctx.values.add(offset) = value };
        }
        return;
    }
}

impl PluginCore for Lv2Plugin {
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
        self.parameters.len()
    }
    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        self.parameters
            .get(index)
            .cloned()
            .ok_or(Error::InvalidParameter(index))
    }
    fn parameter_value(&self, index: usize) -> Result<f64> {
        let info = self.parameter_info(index)?;
        let offset = *self
            .control_value_offset
            .get(info.id as usize)
            .ok_or(Error::InvalidParameter(index))?;
        self.control_values
            .get(offset)
            .map(|value| f64::from(*value))
            .ok_or(Error::InvalidParameter(index))
    }
    fn parameter_value_string(&self, index: usize, value: f64) -> Result<String> {
        let info = self.parameter_info(index)?;
        if info.unit.is_empty() {
            Ok(format!("{value:.3}"))
        } else {
            Ok(format!("{value:.3} {}", info.unit))
        }
    }
    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        let info = self.parameter_info(index)?;
        let offset = *self
            .control_value_offset
            .get(info.id as usize)
            .ok_or(Error::InvalidParameter(index))?;
        let slot = self
            .control_values
            .get_mut(offset)
            .ok_or(Error::InvalidParameter(index))?;
        *slot = value.clamp(info.min, info.max) as f32;
        Ok(())
    }
    fn preset_count(&self) -> usize {
        0
    }
    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        Err(Error::InvalidParameter(index))
    }
    fn load_preset(&mut self, _preset_number: i32) -> Result<()> {
        Err(Error::Other("lv2 preset loading not yet wired".into()))
    }
    fn save_state(&self) -> Result<Vec<u8>> {
        if self.instance.is_null() {
            return Err(Error::Other(
                "lv2 state save requires an active instance".into(),
            ));
        }
        let state_uri = CString::new(format!("{}#state", self.info.unique_id)).map_err(|_| {
            Error::Other("lv2 plugin URI contains NUL; cannot build state URI".into())
        })?;
        let mut ctx = SavePortValues {
            ports: &self.ports,
            offsets: &self.control_value_offset,
            values: &self.control_values,
            scratch: 0.0,
            float_type: self.time_urids.atom_float,
        };
        let ctx_ptr = (&mut ctx as *mut SavePortValues).cast::<c_void>();
        let state = unsafe {
            lilv_sys::lilv_state_new_from_instance(
                self.plugin,
                self.instance,
                self.features.map_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                Some(get_port_value),
                ctx_ptr,
                0,
                self.features.feature_ptrs.as_ptr(),
            )
        };
        if state.is_null() {
            return Err(Error::Other(
                "lilv_state_new_from_instance returned NULL".into(),
            ));
        }
        let text = unsafe {
            lilv_sys::lilv_state_to_string(
                self.world.ptr,
                self.features.map_ptr(),
                self.features.unmap_ptr().cast::<c_void>(),
                state,
                state_uri.as_ptr(),
                std::ptr::null(),
            )
        };
        unsafe { lilv_sys::lilv_state_free(state) };
        if text.is_null() {
            return Err(Error::Other("lilv_state_to_string returned NULL".into()));
        }
        let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
        unsafe { libc::free(text.cast()) };
        Ok(bytes)
    }
    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        self.restore_state_bytes(bytes)
    }

    fn activate(
        &mut self,
        layout: BusLayout,
        sample_rate: f64,
        max_block_size: usize,
    ) -> Result<()> {
        // (Re)instantiate if this is the first activate or the
        // sample rate has changed — LV2 bakes the rate into the
        // instance at `lilv_plugin_instantiate` time.
        let needs_reinstantiate =
            self.instance.is_null() || (self.sample_rate - sample_rate).abs() > f64::EPSILON;
        if needs_reinstantiate {
            if !self.instance.is_null() {
                unsafe {
                    lilv_sys::lilv_instance_deactivate(self.instance);
                    lilv_sys::lilv_instance_free(self.instance);
                }
                self.instance = std::ptr::null_mut();
            }
            let inst = unsafe {
                lilv_sys::lilv_plugin_instantiate(
                    self.plugin,
                    sample_rate,
                    self.features.feature_ptrs.as_ptr(),
                )
            };
            if inst.is_null() {
                return Err(Error::Other("lilv_plugin_instantiate returned NULL".into()));
            }
            self.instance = inst;
            self.sample_rate = sample_rate;
        }

        // A state loaded before activation is applied once the instance exists.
        if let Some(bytes) = self.pending_state.take() {
            self.restore_state_bytes(&bytes)?;
        }

        // Rough sizing: room for a sequence header plus one max-len
        // MIDI event per frame. Plenty for any sane block.
        let event_slot = mem::size_of::<LV2AtomEvent>() + 8;
        let cap = mem::size_of::<LV2AtomSequence>()
            + TIME_POSITION_MAX_BYTES
            + max_block_size * event_slot;
        self.midi_in_buf.resize(cap.max(64), 0);
        self.midi_out_buf.resize(cap.max(64), 0);

        unsafe { lilv_sys::lilv_instance_activate(self.instance) };
        self.active_layout = Some(layout);
        Ok(())
    }
    fn deactivate(&mut self) {
        if !self.instance.is_null() {
            unsafe { lilv_sys::lilv_instance_deactivate(self.instance) };
        }
        self.active_layout = None;
    }
    fn is_active(&self) -> bool {
        self.active_layout.is_some()
    }

    fn latency_samples(&self) -> u32 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        self.latency_offset
            .and_then(|offset| self.control_values.get(offset))
            .map_or(0, |value| value.max(0.0).round() as u32)
    }

    fn editor(&mut self) -> Option<&mut dyn PluginEditor> {
        if self.ui_bundle.is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl Plugin<f32> for Lv2Plugin {
    fn process(
        &mut self,
        buffer: &mut AudioBuffer<'_, f32>,
        events: &EventList,
        context: &mut ProcessContext<'_>,
    ) -> Result<ProcessStatus> {
        if !self.is_active() {
            return Err(Error::NotActivated);
        }
        let frames = buffer.num_frames();

        // Build the input atom sequence (host transport + MIDI) and
        // reset the output.
        self.build_midi_sequence(events, context.transport);
        self.prep_midi_out();

        // Take raw pointers for every audio plane up front so the
        // immutable + mutable borrows on `buffer` don't overlap
        // when we iterate ports below. The slabs live on the plugin
        // so steady-state blocks reuse their capacity.
        self.in_ptrs.clear();
        self.in_ptrs
            .extend(buffer.main_inputs().iter().map(|s| s.as_ptr().cast_mut()));
        self.out_ptrs.clear();
        self.out_ptrs
            .extend(buffer.main_outputs().iter_mut().map(|s| s.as_mut_ptr()));
        let control_base = self.control_values.as_mut_ptr();
        let midi_in_ptr = self.midi_in_buf.as_mut_ptr().cast::<c_void>();
        let midi_out_ptr = self.midi_out_buf.as_mut_ptr().cast::<c_void>();

        // Connect each port. Audio + atom port directions are already
        // classified at load; we just count audio ports as we go to
        // pick the right channel of the host buffer.
        let mut next_audio_in = 0usize;
        let mut next_audio_out = 0usize;
        for port in &self.ports {
            let data: *mut c_void = match port.kind {
                PortKind::Audio => {
                    if port.is_input {
                        let plane = self
                            .in_ptrs
                            .get(next_audio_in)
                            .copied()
                            .unwrap_or(std::ptr::null_mut());
                        next_audio_in += 1;
                        plane.cast::<c_void>()
                    } else {
                        let plane = self
                            .out_ptrs
                            .get(next_audio_out)
                            .copied()
                            .unwrap_or(std::ptr::null_mut());
                        next_audio_out += 1;
                        plane.cast::<c_void>()
                    }
                }
                PortKind::Control { .. } => {
                    let off = self.control_value_offset[port.index as usize];
                    if off == usize::MAX {
                        std::ptr::null_mut()
                    } else {
                        // SAFETY: `control_base` is the head of the
                        // `control_values` Vec; `off` was computed
                        // from its push order at load time.
                        unsafe { control_base.add(off).cast::<c_void>() }
                    }
                }
                PortKind::AtomSequence => {
                    if port.is_input {
                        midi_in_ptr
                    } else {
                        midi_out_ptr
                    }
                }
                PortKind::Other => std::ptr::null_mut(),
            };
            unsafe {
                lilv_sys::lilv_instance_connect_port(self.instance, port.index, data);
            }
        }

        unsafe {
            lilv_sys::lilv_instance_run(self.instance, u32::try_from(frames).unwrap_or(u32::MAX));
        }

        // Drain MIDI from the output atom-sequence port (if the
        // plugin connected one) into context.output_events.
        self.drain_midi_out(context);

        Ok(ProcessStatus::Continue)
    }
}

impl Lv2Plugin {
    /// Walk the MIDI output atom-sequence buffer the plugin just
    /// wrote and translate every `MidiEvent`-typed event back into
    /// rack2-core `EventList` events on `context.output_events`.
    fn drain_midi_out(&mut self, context: &mut ProcessContext<'_>) {
        let header_size = mem::size_of::<LV2AtomSequence>();
        if self.midi_out_buf.len() < header_size {
            return;
        }
        let buf = self.midi_out_buf.as_ptr();
        // SAFETY: midi_out_buf is sized for an LV2AtomSequence header
        // plus events at activate. Vec<u8> is heap-aligned to >= 8.
        let seq = unsafe { &*buf.cast::<LV2AtomSequence>() };
        // Sequence body size (atom.size) excludes the LV2Atom header
        // itself; the body's events run from sequence_begin to
        // sequence_end.
        let body_size = seq.atom.size;
        let event_header = mem::size_of::<LV2AtomEvent>();
        let body_offset = mem::size_of::<LV2Atom>();
        let mut cursor = body_offset + mem::size_of::<LV2AtomSequenceBody>();
        let body_end = body_offset + body_size as usize;
        while cursor + event_header <= body_end && cursor + event_header <= self.midi_out_buf.len()
        {
            let ev_ptr = unsafe { buf.add(cursor) }.cast::<LV2AtomEvent>();
            // SAFETY: cursor + event_header <= midi_out_buf.len().
            let ev = unsafe { &*ev_ptr };
            let payload_size = ev.body.size as usize;
            let payload_total = event_header + payload_size;
            if cursor + payload_total > body_end {
                break;
            }
            if ev.body.mytype == self.midi_event_urid && (1..=8).contains(&payload_size) {
                let data_ptr = unsafe { (ev_ptr as *const u8).add(event_header) };
                // SAFETY: payload_size bytes immediately follow the
                // event header within the bounds we just checked.
                let bytes = unsafe { std::slice::from_raw_parts(data_ptr, payload_size) };
                if let Some(body) = decode_midi_atom(bytes) {
                    let offset = u32::try_from(ev.time_in_frames.max(0)).unwrap_or(0);
                    context.output_events.push(Event {
                        sample_offset: offset,
                        body,
                    });
                }
            }
            // Advance cursor past this event, padded to 8 bytes per
            // the atom alignment rule.
            cursor += (payload_total + 7) & !7;
        }
    }
}
