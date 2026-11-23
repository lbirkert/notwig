use notwig_core::dsp::{DeviceProcessor, ProcessContext, TransportInfo};
use notwig_core::{DeviceId, ParamId, ParamValue, TrackId};

use super::ActivePlan;
use crate::input::{InputBus, MAX_INPUT_TRACKS};

pub(super) struct LowLatencyMonitorRuntime {
    roots: Vec<(DeviceId, Box<dyn DeviceProcessor>)>,
    buffer: Vec<f32>,
}

impl LowLatencyMonitorRuntime {
    pub(super) fn new(roots: Vec<(DeviceId, Box<dyn DeviceProcessor>)>, block_size: usize) -> Self {
        Self {
            roots,
            buffer: vec![0.0; block_size.max(1) * 2],
        }
    }

    pub(super) fn reset(&mut self) {
        for (_, processor) in &mut self.roots {
            processor.reset();
        }
        self.buffer.fill(0.0);
    }

    pub(super) fn set_param(&mut self, device_id: DeviceId, param_id: ParamId, value: &ParamValue) {
        let mut updated_root = false;
        for (root_id, processor) in &mut self.roots {
            if *root_id == device_id {
                processor.set_param(param_id, value);
                updated_root = true;
            }
        }
        if !updated_root {
            for (_, processor) in &mut self.roots {
                if processor.set_nested_param(device_id, param_id, value) {
                    break;
                }
            }
        }
    }
}

impl ActivePlan {
    pub(crate) fn render_low_latency_monitor(
        &mut self,
        input_bus: &InputBus,
        input_routes: &[Option<(TrackId, u16)>; MAX_INPUT_TRACKS],
        frames: usize,
        output: &mut [f32],
        sample_position: u64,
        playing: bool,
    ) {
        self.runtime_faults.clear();
        output[..frames * 2].fill(0.0);
        if !self.low_latency_monitoring {
            return;
        }
        let config = self.config;
        let tempo_bpm = self.tempo_bpm;
        let map = notwig_core::TempoMap {
            tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
        let transport = TransportInfo {
            playhead_mus: map.samples_to_mus(sample_position),
            loop_start_mus: None,
            loop_end_mus: None,
        };
        let ctx = ProcessContext {
            config,
            sample_position,
            playing,
            tempo_bpm,
            transport: &transport,
        };
        for (track_id, slot) in input_routes.iter().flatten() {
            let Some(track) = self.tracks.get(track_id) else {
                continue;
            };
            if track.monitor != notwig_core::MonitorMode::LowLatency {
                continue;
            }
            let gains = track.monitor_gain(true);
            let Some(monitor) = self.low_monitors.get_mut(track_id) else {
                input_bus.discard_monitor(*slot, frames);
                continue;
            };
            let mut faults = [None; super::MAX_RUNTIME_FAULTS];
            let mut fault_count = 0usize;
            let buffer = &mut monitor.buffer[..frames * 2];
            buffer.fill(0.0);
            input_bus.drain_monitor_into(*slot, frames, buffer);
            for (device_id, processor) in &mut monitor.roots {
                let status = processor.process(&ctx, buffer, &[]);
                if status.produced_silence {
                    buffer.fill(0.0);
                }
                if status.overloaded {
                    for sample in buffer.iter_mut() {
                        if !sample.is_finite() {
                            *sample = 0.0;
                        }
                    }
                    if fault_count < faults.len() {
                        faults[fault_count] = Some(*device_id);
                        fault_count += 1;
                    }
                }
            }
            for frame in buffer.chunks_exact_mut(2) {
                frame[0] *= gains.0;
                frame[1] *= gains.1;
            }
            for (dst, sample) in output[..frames * 2].iter_mut().zip(buffer.iter()) {
                *dst += *sample;
            }
            for device_id in faults[..fault_count].iter().flatten() {
                if self.runtime_faults.len() < super::MAX_RUNTIME_FAULTS {
                    self.runtime_faults.push(super::RuntimeFault {
                        device_id: *device_id,
                    });
                }
            }
        }
    }
}
