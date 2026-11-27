//! Core VST 2 plugin instance: loading, parameters, state, and processing.

use crate::abi::{self, AEffect, EventList as VstEventList, MAX_EVENTS, MidiEvent, SysExEvent};
use crate::host::{CURRENT_ID, HostShared, ProcessLevel, host_callback};
use crate::loader::LoadedModule;
use crate::midi::{midi_to_bytes, push_midi_event, push_sysex_event};
use crate::scan::{FORMAT, plugin_directory, resolve_binary};
use crate::{DEFAULT_BLOCK_SIZE, DEFAULT_SAMPLE_RATE, MAX_CHANNELS, MAX_STATE_BYTES};
use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::bus::{Bus, BusLayout, ChannelConfig};
use notwig_rack_core::editor::PluginEditor;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::{EventBody, EventList};
use notwig_rack_core::info::{
    ParameterFlags, ParameterInfo, PluginCategory, PluginInfo, PresetInfo,
};
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};
use notwig_rack_core::wrapper::run_audio_block_with;
use std::cell::Cell;
use std::ffi::c_void;
use std::path::Path;
use std::ptr;
use std::sync::atomic::Ordering;

/// One loaded VST 2 plugin instance.
pub struct Vst2Plugin {
    info: PluginInfo,
    layouts: Vec<BusLayout>,
    active_layout: Option<BusLayout>,
    #[allow(dead_code)]
    module: LoadedModule,
    effect: *mut AEffect,
    pub(crate) host: Box<HostShared>,
    parameter_count: usize,
    num_inputs: usize,
    num_outputs: usize,
    sample_rate: f64,
    max_block_size: usize,
    input_silence: Vec<Vec<f32>>,
    output_scratch: Vec<Vec<f32>>,
    active: bool,
    pub(crate) editor_open: bool,
    pub(crate) editor_size: Cell<Option<(u32, u32)>>,
}

// SAFETY: a plugin instance is owned by one thread at a time (the
// `Plugin` trait takes `&mut self`), and the loaded module stays
// mapped for the instance's lifetime. The raw AEffect pointer is
// never shared without that exclusive borrow.
unsafe impl Send for Vst2Plugin {}

impl Vst2Plugin {
    pub(crate) fn load_path(path: &Path, child_id: i32) -> Result<Self> {
        let info = PluginInfo {
            name: String::new(),
            vendor: String::new(),
            version: 0,
            category: PluginCategory::Effect,
            path: path.to_path_buf(),
            unique_id: child_id.to_string(),
            format: FORMAT,
            has_editor: false,
            accepts_midi: false,
            supports_ara: false,
        };
        Self::load_from(&info)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn load_from(info: &PluginInfo) -> Result<Self> {
        let binary = resolve_binary(&info.path);
        let module = unsafe { LoadedModule::open(&info.path, &binary) }?;

        let child_id = info.unique_id.parse::<i32>().unwrap_or(0);
        let host = Box::new(HostShared::new(plugin_directory(&info.path)));
        let entry = unsafe { module.entry() }?;
        let effect = {
            let previous = CURRENT_ID.replace(child_id);
            let effect = unsafe { entry(host_callback) };
            CURRENT_ID.set(previous);
            effect
        };

        if effect.is_null() {
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: "VSTPluginMain returned null".into(),
            });
        }

        let fields = unsafe { effect.read() };
        if let Err(err) = validate_effect(&fields, &info.path) {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(err);
        }

        // resvd1 is host-owned storage; the plugin must not touch it.
        unsafe {
            let host_ptr = (&raw const *host).cast_mut().cast::<c_void>();
            (*effect).resvd1 = host_ptr;
        }

        let _ = dispatch(effect, abi::EFF_OPEN, 0, 0, ptr::null_mut(), 0.0);
        let fields = unsafe { effect.read() };
        if let Err(err) = validate_effect(&fields, &info.path) {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(err);
        }
        if child_id != 0 && fields.unique_id != 0 && fields.unique_id != child_id {
            if let Some(close) = fields.dispatcher {
                unsafe { close(effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
            }
            return Err(Error::LoadFailed {
                path: info.path.clone(),
                reason: format!(
                    "VST2 shell requested id {child_id}, module reported {}",
                    fields.unique_id
                ),
            });
        }

        let parameter_count = usize::try_from(fields.num_params).unwrap_or(0);
        let num_inputs = usize::try_from(fields.num_inputs).unwrap_or(0);
        let num_outputs = usize::try_from(fields.num_outputs).unwrap_or(0);

        let mut info = info.clone();
        if info.name.is_empty() {
            info.name = dispatch_text(effect, abi::EFF_GET_EFFECT_NAME, 0);
        }
        if info.vendor.is_empty() {
            info.vendor = dispatch_text(effect, abi::EFF_GET_VENDOR_STRING, 0);
        }
        if info.unique_id.is_empty() || info.unique_id == "0" {
            info.unique_id = fields.unique_id.to_string();
        }
        info.version = u32::try_from(fields.version).unwrap_or(0);
        info.category = effect_category(effect, &fields);
        info.has_editor = fields.flags & abi::FLAG_HAS_EDITOR != 0;
        info.accepts_midi = fields.flags & abi::FLAG_IS_SYNTH != 0
            || dispatch(
                effect,
                abi::EFF_CAN_DO,
                0,
                0,
                c"receiveVstMidiEvent".as_ptr().cast_mut().cast(),
                0.0,
            ) == 1;
        info.format = FORMAT;

        Ok(Self {
            info,
            layouts: vec![layout_for(num_inputs, num_outputs)],
            active_layout: None,
            module,
            effect,
            host,
            parameter_count,
            num_inputs,
            num_outputs,
            sample_rate: DEFAULT_SAMPLE_RATE,
            max_block_size: DEFAULT_BLOCK_SIZE,
            input_silence: Vec::new(),
            output_scratch: Vec::new(),
            active: false,
            editor_open: false,
            editor_size: Cell::new(None),
        })
    }

    pub(crate) fn class_infos(&self) -> Vec<PluginInfo> {
        let category = dispatch(
            self.effect,
            abi::EFF_GET_PLUG_CATEGORY,
            0,
            0,
            ptr::null_mut(),
            0.0,
        );
        if category as i32 != abi::PLUG_CATEG_SHELL {
            return vec![self.info.clone()];
        }

        let mut out = Vec::new();
        for _ in 0..1024 {
            let mut name = [0u8; 256];
            let id = dispatch(
                self.effect,
                abi::EFF_SHELL_GET_NEXT_PLUGIN,
                0,
                0,
                name.as_mut_ptr().cast(),
                0.0,
            ) as i32;
            if id == 0 {
                break;
            }
            let unique_id = id.to_string();
            if out
                .iter()
                .any(|entry: &PluginInfo| entry.unique_id == unique_id)
            {
                break;
            }
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            out.push(PluginInfo {
                name: String::from_utf8_lossy(&name[..end]).into_owned(),
                vendor: self.info.vendor.clone(),
                version: self.info.version,
                category: PluginCategory::Effect,
                path: self.info.path.clone(),
                unique_id,
                format: FORMAT,
                has_editor: self.info.has_editor,
                accepts_midi: self.info.accepts_midi,
                supports_ara: false,
            });
        }

        if out.is_empty() {
            vec![self.info.clone()]
        } else {
            out
        }
    }

    pub(crate) fn dispatch(
        &self,
        opcode: i32,
        index: i32,
        value: isize,
        pointer: *mut c_void,
        option: f32,
    ) -> isize {
        dispatch(self.effect, opcode, index, value, pointer, option)
    }

    fn text(&self, opcode: i32, index: i32) -> String {
        dispatch_text(self.effect, opcode, index)
    }
}

impl Drop for Vst2Plugin {
    fn drop(&mut self) {
        if self.editor_open {
            self.close();
        }
        if self.active {
            self.deactivate();
        }
        if let Some(close) = unsafe { (*self.effect).dispatcher } {
            unsafe { close(self.effect, abi::EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0) };
        }
    }
}

fn validate_effect(fields: &AEffect, path: &Path) -> Result<()> {
    if fields.magic != abi::MAGIC || fields.dispatcher.is_none() {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "invalid VST2 AEffect header".into(),
        });
    }
    if !(0..=64).contains(&fields.num_inputs)
        || !(0..=64).contains(&fields.num_outputs)
        || !(0..=16384).contains(&fields.num_params)
    {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "VST2 channel or parameter count out of range".into(),
        });
    }
    if fields.process_replacing.is_none() && fields.process.is_none() {
        return Err(Error::LoadFailed {
            path: path.to_path_buf(),
            reason: "VST2 plugin exposes no 32-bit process callback".into(),
        });
    }
    Ok(())
}

fn effect_category(effect: *mut AEffect, fields: &AEffect) -> PluginCategory {
    if fields.flags & abi::FLAG_IS_SYNTH != 0 {
        return PluginCategory::Instrument;
    }
    match dispatch(
        effect,
        abi::EFF_GET_PLUG_CATEGORY,
        0,
        0,
        ptr::null_mut(),
        0.0,
    ) as i32
    {
        abi::PLUG_CATEG_SYNTH => PluginCategory::Instrument,
        abi::PLUG_CATEG_EFFECT => PluginCategory::Effect,
        _ => {
            if fields.flags & abi::FLAG_IS_EFFECT != 0 {
                PluginCategory::Effect
            } else {
                PluginCategory::Tool
            }
        }
    }
}

fn channel_config(channels: usize) -> ChannelConfig {
    match channels {
        0 => ChannelConfig::Discrete(0),
        1 => ChannelConfig::Mono,
        2 => ChannelConfig::Stereo,
        6 => ChannelConfig::Surround5_1,
        8 => ChannelConfig::Surround7_1,
        other => ChannelConfig::Discrete(u32::try_from(other).unwrap_or(u32::MAX)),
    }
}

pub(crate) fn layout_for(inputs: usize, outputs: usize) -> BusLayout {
    let mut layout = BusLayout::new();
    if inputs > 0 {
        layout
            .inputs
            .push(Bus::main("Input", channel_config(inputs)));
    }
    if outputs > 0 {
        layout
            .outputs
            .push(Bus::main("Output", channel_config(outputs)));
    }
    layout
}

impl PluginCore for Vst2Plugin {
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
        self.parameter_count
    }

    fn parameter_info(&self, index: usize) -> Result<ParameterInfo> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let index_i32 = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        let name = self.text(abi::EFF_GET_PARAM_NAME, index_i32);
        let unit = self.text(abi::EFF_GET_PARAM_LABEL, index_i32);
        let default = unsafe {
            (*self.effect)
                .get_parameter
                .map_or(0.0, |get| f64::from(get(self.effect, index as u32)))
        };
        let mut flags = ParameterFlags::empty();
        if unsafe { (*self.effect).set_parameter.is_none() } {
            flags |= ParameterFlags::READ_ONLY;
        }
        if self.dispatch(
            abi::EFF_CAN_BE_AUTOMATED,
            index_i32,
            0,
            ptr::null_mut(),
            0.0,
        ) != 0
        {
            flags |= ParameterFlags::AUTOMATABLE;
        }
        Ok(ParameterInfo {
            id: u32::try_from(index).unwrap_or(u32::MAX),
            name: name.clone(),
            short_name: name,
            unit,
            min: 0.0,
            max: 1.0,
            default,
            step_count: 0,
            flags,
        })
    }

    fn parameter_value(&self, index: usize) -> Result<f64> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let get = unsafe { (*self.effect).get_parameter }
            .ok_or_else(|| Error::Other("VST2 plugin has no getParameter".into()))?;
        Ok(f64::from(unsafe { get(self.effect, index as u32) }))
    }

    fn parameter_value_string(&self, index: usize, value: f64) -> Result<String> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let text = self.text(
            abi::EFF_GET_PARAM_DISPLAY,
            i32::try_from(index).unwrap_or(0),
        );
        if text.is_empty() {
            Ok(format!("{value:.3}"))
        } else {
            Ok(text)
        }
    }

    fn set_parameter(&mut self, index: usize, value: f64) -> Result<()> {
        if index >= self.parameter_count {
            return Err(Error::InvalidParameter(index));
        }
        let set = unsafe { (*self.effect).set_parameter }
            .ok_or_else(|| Error::Other("VST2 plugin has no setParameter".into()))?;
        let value = value.clamp(0.0, 1.0) as f32;
        unsafe { set(self.effect, index as u32, value) };
        Ok(())
    }

    fn preset_count(&self) -> usize {
        usize::try_from(unsafe { (*self.effect).num_programs }).unwrap_or(0)
    }

    fn preset_info(&self, index: usize) -> Result<PresetInfo> {
        if index >= self.preset_count() {
            return Err(Error::InvalidParameter(index));
        }
        let index_i32 = i32::try_from(index).map_err(|_| Error::InvalidParameter(index))?;
        let mut name = self.text(abi::EFF_GET_PROGRAM_NAME_INDEXED, index_i32);
        if name.is_empty() {
            name = self.text(abi::EFF_GET_PROGRAM_NAME, 0);
        }
        Ok(PresetInfo {
            index,
            name,
            preset_number: index_i32,
        })
    }

    fn load_preset(&mut self, preset_number: i32) -> Result<()> {
        if preset_number < 0
            || usize::try_from(preset_number).unwrap_or(usize::MAX) >= self.preset_count()
        {
            return Err(Error::InvalidParameter(
                usize::try_from(preset_number).unwrap_or(usize::MAX),
            ));
        }
        self.dispatch(
            abi::EFF_SET_PROGRAM,
            0,
            preset_number as isize,
            ptr::null_mut(),
            0.0,
        );
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>> {
        let fields = unsafe { self.effect.read() };
        if fields.flags & abi::FLAG_PROGRAM_CHUNKS != 0 {
            let mut data: *const u8 = ptr::null();
            let size = self.dispatch(abi::EFF_GET_CHUNK, 0, 0, (&raw mut data).cast(), 0.0);
            if size < 0 || usize::try_from(size).unwrap_or(usize::MAX) > MAX_STATE_BYTES {
                return Err(Error::Other(
                    "VST2 effGetChunk returned an invalid size".into(),
                ));
            }
            if size == 0 {
                return Ok(Vec::new());
            }
            if data.is_null() {
                return Err(Error::Other(
                    "VST2 effGetChunk returned a null pointer".into(),
                ));
            }
            return Ok(unsafe { std::slice::from_raw_parts(data, size as usize) }.to_vec());
        }

        let program = if fields.num_programs > 0 {
            self.dispatch(abi::EFF_GET_PROGRAM, 0, 0, ptr::null_mut(), 0.0) as i32
        } else {
            0
        };
        let get = fields
            .get_parameter
            .ok_or_else(|| Error::Other("VST2 plugin has no getParameter".into()))?;
        let mut out = Vec::with_capacity(4 + self.parameter_count * 4);
        out.extend_from_slice(&program.to_le_bytes());
        for index in 0..self.parameter_count {
            let value = unsafe { get(self.effect, index as u32) };
            out.extend_from_slice(&value.to_le_bytes());
        }
        Ok(out)
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_STATE_BYTES {
            return Err(Error::Other("VST2 state blob too large".into()));
        }
        let fields = unsafe { self.effect.read() };
        if fields.flags & abi::FLAG_PROGRAM_CHUNKS != 0 {
            let mut blob = bytes.to_vec();
            self.dispatch(
                abi::EFF_SET_CHUNK,
                0,
                blob.len() as isize,
                blob.as_mut_ptr().cast(),
                0.0,
            );
            return Ok(());
        }

        let expected = 4 + self.parameter_count * 4;
        if bytes.len() != expected {
            return Err(Error::Other(format!(
                "VST2 parameter state has {} bytes, expected {expected}",
                bytes.len()
            )));
        }
        let program =
            i32::from_le_bytes(bytes[..4].try_into().map_err(|_| {
                Error::Other("VST2 parameter state has a short program number".into())
            })?);
        if fields.num_programs > 0 {
            if program < 0 || program >= fields.num_programs {
                return Err(Error::Other(
                    "VST2 parameter state has an invalid program".into(),
                ));
            }
            self.dispatch(
                abi::EFF_SET_PROGRAM,
                0,
                program as isize,
                ptr::null_mut(),
                0.0,
            );
        }

        let set = fields
            .set_parameter
            .ok_or_else(|| Error::Other("VST2 plugin has no setParameter".into()))?;
        for (index, chunk) in bytes[4..].as_chunks::<4>().0.iter().enumerate() {
            let value = f32::from_le_bytes(*chunk);
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(Error::Other(
                    "VST2 parameter state contains an invalid value".into(),
                ));
            }
            unsafe { set(self.effect, index as u32, value) };
        }
        Ok(())
    }

    fn activate(
        &mut self,
        layout: BusLayout,
        sample_rate: f64,
        max_block_size: usize,
    ) -> Result<()> {
        if self.active {
            self.deactivate();
        }
        self.dispatch(abi::EFF_SET_PROCESS_PRECISION, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(
            abi::EFF_SET_SAMPLE_RATE,
            0,
            0,
            ptr::null_mut(),
            sample_rate as f32,
        );
        self.dispatch(
            abi::EFF_SET_BLOCK_SIZE,
            0,
            max_block_size as isize,
            ptr::null_mut(),
            0.0,
        );
        self.dispatch(abi::EFF_MAINS_CHANGED, 0, 1, ptr::null_mut(), 0.0);
        self.dispatch(abi::EFF_START_PROCESS, 0, 0, ptr::null_mut(), 0.0);

        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        let block = max_block_size.max(1);
        self.input_silence = (0..self.num_inputs).map(|_| vec![0.0; block]).collect();
        self.output_scratch = (0..self.num_outputs).map(|_| vec![0.0; block]).collect();
        self.host
            .sample_rate
            .store(sample_rate.to_bits(), Ordering::Relaxed);
        self.host
            .block_size
            .store(max_block_size, Ordering::Relaxed);
        self.active = true;
        self.active_layout = Some(layout);
        Ok(())
    }

    fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        self.dispatch(abi::EFF_STOP_PROCESS, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(abi::EFF_MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
        self.active = false;
        self.active_layout = None;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn latency_samples(&self) -> u32 {
        let delay = unsafe { (*self.effect).initial_delay };
        u32::try_from(delay.max(0)).unwrap_or(0)
    }

    fn editor(&mut self) -> Option<&mut dyn PluginEditor> {
        if self.info.has_editor {
            Some(self)
        } else {
            None
        }
    }
}

impl Plugin<f32> for Vst2Plugin {
    #[allow(clippy::too_many_lines)]
    fn process(
        &mut self,
        buffer: &mut AudioBuffer<'_, f32>,
        events: &EventList,
        context: &mut ProcessContext<'_>,
    ) -> Result<ProcessStatus> {
        if !self.active {
            return Err(Error::NotActivated);
        }
        let frames = buffer.num_frames();
        if frames == 0 {
            return Ok(ProcessStatus::Continue);
        }

        self.host
            .sample_rate
            .store(context.sample_rate.to_bits(), Ordering::Relaxed);
        self.host
            .block_size
            .store(self.max_block_size, Ordering::Relaxed);
        self.host.time.update(context.transport);

        let mut input_ptrs = [ptr::null::<f32>(); MAX_CHANNELS];
        {
            let inputs = buffer.main_inputs();
            let provided = inputs.len().min(self.num_inputs);
            for (slot, channel) in input_ptrs.iter_mut().zip(inputs.iter()).take(provided) {
                *slot = channel.as_ptr();
            }
            for (index, slot) in input_ptrs
                .iter_mut()
                .enumerate()
                .take(self.num_inputs)
                .skip(provided)
            {
                let Some(channel) = self.input_silence.get(index) else {
                    return Err(Error::Other("VST2 input scratch is not allocated".into()));
                };
                if channel.len() < frames {
                    return Err(Error::Other("VST2 input scratch is too small".into()));
                }
                *slot = channel.as_ptr();
            }
        }

        let mut output_ptrs = [ptr::null_mut::<f32>(); MAX_CHANNELS];
        {
            let outputs = buffer.main_outputs();
            let provided = outputs.len().min(self.num_outputs);
            for (slot, channel) in output_ptrs
                .iter_mut()
                .zip(outputs.iter_mut())
                .take(provided)
            {
                *slot = channel.as_mut_ptr();
            }
            for (index, slot) in output_ptrs
                .iter_mut()
                .enumerate()
                .take(self.num_outputs)
                .skip(provided)
            {
                let Some(channel) = self.output_scratch.get_mut(index) else {
                    return Err(Error::Other("VST2 output scratch is not allocated".into()));
                };
                if channel.len() < frames {
                    return Err(Error::Other("VST2 output scratch is too small".into()));
                }
                *slot = channel.as_mut_ptr();
            }
        }

        let mut midi_events = [MidiEvent::default(); MAX_EVENTS];
        let mut sysex_events = [SysExEvent::default(); MAX_EVENTS];
        let mut sysex_bytes = [[0u8; 8]; MAX_EVENTS];
        let mut list = VstEventList::default();

        for event in events {
            match event.body {
                EventBody::Midi(midi) => {
                    if let Some(raw) = midi_to_bytes(midi) {
                        if raw.len <= 3 {
                            push_midi_event(
                                &mut list,
                                &mut midi_events,
                                event.sample_offset,
                                &raw.data,
                            );
                        } else {
                            push_sysex_event(
                                &mut list,
                                &mut sysex_events,
                                &mut sysex_bytes,
                                event.sample_offset,
                                &raw.data[..usize::from(raw.len)],
                            );
                        }
                    }
                }
                EventBody::ParamValue { param_id, value } => {
                    if let Some(set) = unsafe { (*self.effect).set_parameter }
                        && (param_id as usize) < self.parameter_count
                    {
                        unsafe { set(self.effect, param_id, value.clamp(0.0, 1.0) as f32) };
                    }
                }
                EventBody::ParamGesture { .. } | EventBody::TransportFlag(_) => {}
            }
        }

        if list.num_events > 0 {
            let _ = self.dispatch(abi::EFF_PROCESS_EVENTS, 0, 0, (&raw mut list).cast(), 0.0);
        }

        let _level = ProcessLevel::enter();
        let effect = self.effect;
        let num_inputs = self.num_inputs;
        let num_outputs = self.num_outputs;
        let ok = run_audio_block_with::<Vst2Plugin, bool>(FORMAT, false, || unsafe {
            if let Some(process) = (*effect).process_replacing {
                process(
                    effect,
                    input_ptrs.as_ptr(),
                    output_ptrs.as_mut_ptr(),
                    frames as i32,
                );
                true
            } else if let Some(process) = (*effect).process {
                for index in 0..num_outputs {
                    if index < num_inputs && !input_ptrs[index].is_null() {
                        ptr::copy_nonoverlapping(input_ptrs[index], output_ptrs[index], frames);
                    } else if !output_ptrs[index].is_null() {
                        ptr::write_bytes(output_ptrs[index], 0, frames);
                    }
                }
                process(
                    effect,
                    input_ptrs.as_ptr(),
                    output_ptrs.as_mut_ptr(),
                    frames as i32,
                );
                true
            } else {
                false
            }
        });

        while let Some(event) = self.host.output_events.pop() {
            context.output_events.push(event);
        }

        if ok {
            Ok(ProcessStatus::Continue)
        } else {
            for pointer in output_ptrs.iter().take(self.num_outputs) {
                if !pointer.is_null() {
                    unsafe { ptr::write_bytes(*pointer, 0, frames) };
                }
            }
            Ok(ProcessStatus::Error)
        }
    }
}

fn dispatch(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    pointer: *mut c_void,
    option: f32,
) -> isize {
    if effect.is_null() {
        return 0;
    }
    let Some(dispatcher) = (unsafe { (*effect).dispatcher }) else {
        return 0;
    };
    unsafe { dispatcher(effect, opcode, index, value, pointer, option) }
}

fn dispatch_text(effect: *mut AEffect, opcode: i32, index: i32) -> String {
    let mut buffer = [0u8; 256];
    dispatch(effect, opcode, index, 0, buffer.as_mut_ptr().cast(), 0.0);
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}
