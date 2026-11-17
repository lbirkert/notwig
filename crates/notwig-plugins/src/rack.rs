use std::collections::BTreeMap;

use notwig_core::{ParamId, ParamValue};
use notwig_dsp::{
    DeviceProcessor, GraphError, Latency, NormMap, NoteKind, ParamFlags, ParamKind, ParamSpec,
    ProcessContext, ProcessStatus, RangeSpec, RenderConfig, Unit,
};
use truce_rack::core::buffer::BusRange;
use truce_rack::core::{
    AudioBuffer, BusLayout, Event, EventBody, EventList, MidiData, Plugin,
    ProcessContext as RackContext, ProcessStatus as RackStatus, TransportInfo,
};
use uuid::Uuid;

use crate::types::{
    PluginCapabilities, PluginDescriptor, PluginError, PluginInstance, PluginState,
};

pub(crate) struct RackPluginInstance {
    plugin: Box<dyn Plugin<f32>>,
    descriptor: PluginDescriptor,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    params: BTreeMap<u32, usize>,
    active: bool,
}

impl RackPluginInstance {
    pub(crate) fn new(plugin: Box<dyn Plugin<f32>>, descriptor: PluginDescriptor) -> Self {
        let mut params = BTreeMap::new();
        for index in 0..plugin.parameter_count() {
            if let Ok(info) = plugin.parameter_info(index) {
                params.insert(info.id, index);
            }
        }
        Self {
            plugin,
            descriptor,
            input: [Vec::new(), Vec::new()],
            output: [Vec::new(), Vec::new()],
            params,
            active: false,
        }
    }

    fn deactivate(&mut self) {
        if self.active {
            self.plugin.deactivate();
            self.active = false;
        }
    }
}

impl DeviceProcessor for RackPluginInstance {
    fn prepare(&mut self, config: RenderConfig) -> Result<(), GraphError> {
        self.deactivate();
        let layout = self
            .plugin
            .supported_layouts()
            .iter()
            .find(|layout| {
                layout.total_input_channels() == 2 && layout.total_output_channels() == 2
            })
            .cloned()
            .unwrap_or_else(BusLayout::stereo);
        self.plugin
            .activate(layout, f64::from(config.sample_rate), config.max_block_size)
            .map_err(|err| GraphError::InvalidDocument {
                reason: err.to_string(),
            })?;
        for channel in &mut self.input {
            channel.resize(config.max_block_size, 0.0);
        }
        for channel in &mut self.output {
            channel.resize(config.max_block_size, 0.0);
        }
        self.active = true;
        Ok(())
    }

    fn reset(&mut self) {
        for channel in &mut self.input {
            channel.fill(0.0);
        }
        for channel in &mut self.output {
            channel.fill(0.0);
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        let raw = param_id.into_uuid().as_u128() as u32;
        if let (Some(index), Some(value)) = (self.params.get(&raw).copied(), value.as_f64()) {
            let _ = self.plugin.set_parameter(index, value);
        }
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[notwig_dsp::NoteEvent],
    ) -> ProcessStatus {
        let frames = audio.len() / 2;
        if frames == 0 {
            return ProcessStatus::default();
        }
        if self.input[0].len() < frames || self.output[0].len() < frames {
            audio.fill(0.0);
            return ProcessStatus {
                overloaded: true,
                produced_silence: true,
            };
        }

        for frame in 0..frames {
            self.input[0][frame] = audio[frame * 2];
            self.input[1][frame] = audio[frame * 2 + 1];
            self.output[0][frame] = 0.0;
            self.output[1][frame] = 0.0;
        }

        let input_refs: [&[f32]; 2] = [&self.input[0][..frames], &self.input[1][..frames]];
        let (out_left, out_right) = self.output.split_at_mut(1);
        let mut output_refs: [&mut [f32]; 2] =
            [&mut out_left[0][..frames], &mut out_right[0][..frames]];
        let input_buses = [BusRange::new(0, 2)];
        let output_buses = [BusRange::new(0, 2)];
        let mut buffer = AudioBuffer::new(
            &input_refs,
            &mut output_refs,
            frames,
            &input_buses,
            &output_buses,
        );

        let event_list = build_events(events);
        let mut output_events = EventList::new();
        let transport = TransportInfo {
            tempo_bpm: Some(ctx.tempo_bpm),
            time_signature: None,
            song_position_beats: Some(ctx.transport.playhead_mus.as_f64()),
            song_position_samples: i64::try_from(ctx.sample_position).ok(),
            bar_start_beats: None,
            playing: ctx.playing,
            recording: false,
            loop_active: ctx.transport.loop_start_mus.is_some(),
        };
        let mut rack_ctx = RackContext {
            sample_rate: f64::from(ctx.config.sample_rate),
            max_block_size: ctx.config.max_block_size,
            transport: Some(transport),
            output_events: &mut output_events,
        };
        let status = self.plugin.process(&mut buffer, &event_list, &mut rack_ctx);

        if status.is_err() {
            audio.fill(0.0);
            return ProcessStatus {
                overloaded: true,
                produced_silence: true,
            };
        }

        for frame in 0..frames {
            audio[frame * 2] = self.output[0][frame];
            audio[frame * 2 + 1] = self.output[1][frame];
        }
        let silence = matches!(status, Ok(RackStatus::Sleep));
        ProcessStatus {
            overloaded: false,
            produced_silence: silence,
        }
    }

    fn latency(&self) -> Latency {
        Latency { samples: 0 }
    }
}

impl PluginInstance for RackPluginInstance {
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            audio_inputs: 2,
            audio_outputs: 2,
            accepts_notes: self.plugin.info().accepts_midi || self.descriptor.is_instrument,
            produces_notes: false,
            has_editor: self.plugin.info().has_editor,
            supports_state: true,
            reports_latency: false,
        }
    }

    fn save_state(&mut self) -> Result<PluginState, PluginError> {
        let blob = self
            .plugin
            .save_state()
            .map_err(|err| PluginError::Load(err.to_string()))?;
        Ok(PluginState {
            format: self.descriptor.format,
            plugin_uid: self.descriptor.uid.clone(),
            plugin_version: self.descriptor.version.clone(),
            blob,
        })
    }

    fn load_state(&mut self, state: &PluginState) -> Result<(), PluginError> {
        if state.format != self.descriptor.format || state.plugin_uid != self.descriptor.uid {
            return Err(PluginError::StateIncompatible(format!(
                "expected {} {}, got {} {}",
                self.descriptor.format.prefix(),
                self.descriptor.uid,
                state.format.prefix(),
                state.plugin_uid
            )));
        }
        self.plugin
            .load_state(&state.blob)
            .map_err(|err| PluginError::Load(err.to_string()))
    }

    fn describe_parameters(&self) -> Vec<ParamSpec> {
        let mut specs = Vec::with_capacity(self.plugin.parameter_count());
        for index in 0..self.plugin.parameter_count() {
            let Ok(info) = self.plugin.parameter_info(index) else {
                continue;
            };
            let id = ParamId::from_uuid(Uuid::from_u128(u128::from(info.id)));
            let is_bool = info.step_count == 1
                && info.min.abs() < f64::EPSILON
                && (info.max - 1.0).abs() < f64::EPSILON;
            let is_enum = info
                .flags
                .contains(truce_rack::core::info::ParameterFlags::ENUMERATED);
            let kind = if is_bool {
                ParamKind::Bool
            } else if is_enum {
                ParamKind::Enum
            } else {
                ParamKind::Continuous
            };
            let range = if is_enum {
                RangeSpec::enumeration(info.step_count.saturating_add(1))
            } else if info.step_count > 0 {
                RangeSpec::stepped(info.min, info.max, 1.0)
            } else {
                RangeSpec::continuous(info.min, info.max)
            };
            specs.push(ParamSpec {
                id,
                name: info.name,
                kind,
                unit: unit_from_name(&info.unit),
                range,
                default: match kind {
                    ParamKind::Bool => ParamValue::Bool(info.default >= 0.5),
                    ParamKind::Enum => ParamValue::EnumIndex(info.default.max(0.0) as u32),
                    ParamKind::Continuous | ParamKind::Discrete => ParamValue::F64(info.default),
                },
                norm_map: NormMap::linear(info.min, info.max),
                smoothing: None,
                flags: ParamFlags {
                    automatable: info
                        .flags
                        .contains(truce_rack::core::info::ParameterFlags::AUTOMATABLE),
                    visible: !info
                        .flags
                        .contains(truce_rack::core::info::ParameterFlags::HIDDEN),
                    modulatable_reserved: true,
                    macro_assignable_reserved: true,
                },
            });
        }
        specs
    }
}

fn build_events(events: &[notwig_dsp::NoteEvent]) -> EventList {
    let mut list = EventList::new();
    for event in events {
        let body = match event.kind {
            NoteKind::On => MidiData::NoteOn {
                channel: 0,
                note: event.pitch,
                velocity: (event.velocity.clamp(0.0, 1.0) * 127.0).round() as u8,
            },
            NoteKind::Off => MidiData::NoteOff {
                channel: 0,
                note: event.pitch,
                velocity: 0,
            },
        };
        list.push(Event {
            sample_offset: event.sample_offset,
            body: EventBody::Midi(body),
        });
    }
    list
}

fn unit_from_name(name: &str) -> Unit {
    match name.trim().to_lowercase().as_str() {
        "db" | "decibel" | "decibels" => Unit::Decibels,
        "hz" | "hertz" => Unit::Hertz,
        "%" | "percent" => Unit::Percent,
        "s" | "sec" | "seconds" => Unit::Seconds,
        "st" | "semitone" | "semitones" => Unit::Semitones,
        _ => Unit::None,
    }
}
