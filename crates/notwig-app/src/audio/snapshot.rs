use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use notwig_core::{DeviceId, RtEvent, TrackId};
use notwig_engine::Engine;
use notwig_ext_api::DeviceMeterSnapshot;
use notwig_ui::MeterValues;

use crate::clock::Instant;

pub(super) const DSP_HISTORY: usize = 200;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioSnapshot {
    pub running: bool,
    pub driver: String,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub block_size: u32,
    pub playhead_samples: u64,
    #[serde(skip)]
    pub playhead_updated: Option<Instant>,
    /// Age of `playhead_samples` at the time the engine process serialized the
    /// snapshot. Used by the UI process to preserve the audio callback's
    /// timestamp across IPC instead of re-timestamping a delayed snapshot.
    #[serde(default)]
    pub playhead_age_nanos: u64,
    pub playing: bool,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub device_meters: BTreeMap<DeviceId, DeviceMeterSnapshot>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub error: Option<String>,
    pub crashed: bool,
    /// The output stream was invalidated because its device disappeared or the
    /// host became unavailable. This is recoverable: the app pauses transport,
    /// rescans devices, and retries opening the configured output.
    #[serde(default)]
    pub device_lost: bool,
    #[serde(default)]
    pub device_lost_reason: Option<String>,
    /// The host rerouted the stream to a different device (for example the OS
    /// default output changed). The stream may still be audible, but the app
    /// should reopen it so settings and device metadata stay in sync.
    #[serde(default)]
    pub device_changed: bool,
    pub dsp: notwig_ui::DspAnalysisView,
    pub(crate) load_ms_history: Vec<f32>,
    pub(crate) jitter_history: Vec<f32>,
}

impl AudioSnapshot {
    /// Meters are only meaningful while transport is running. Stopping or
    /// pausing playback must drop stale peak values instead of leaving the last
    /// active block on screen.
    #[must_use]
    pub fn meters_for_display(&self, playing: bool) -> BTreeMap<TrackId, MeterValues> {
        if self.running && playing {
            self.meters.clone()
        } else {
            BTreeMap::new()
        }
    }

    #[must_use]
    pub fn device_meters_for_display(
        &self,
        playing: bool,
    ) -> BTreeMap<DeviceId, DeviceMeterSnapshot> {
        if self.running && playing {
            self.device_meters.clone()
        } else {
            BTreeMap::new()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DspSample {
    pub(crate) usage_percent: f32,
    pub(crate) load_ms: f32,
    pub(crate) jitter_percent: f32,
    pub(crate) frames: u32,
    pub(crate) output_latency_samples: u64,
}

pub(crate) fn update_snapshot(
    snapshot: &Arc<Mutex<AudioSnapshot>>,
    engine: &Engine,
    events: &[RtEvent],
    dsp: DspSample,
) {
    update_snapshot_at(snapshot, engine, events, engine.playhead_samples(), dsp);
}

pub(crate) fn update_snapshot_at(
    snapshot: &Arc<Mutex<AudioSnapshot>>,
    engine: &Engine,
    events: &[RtEvent],
    playhead_samples: u64,
    dsp: DspSample,
) {
    let Ok(mut snapshot) = snapshot.try_lock() else {
        return;
    };
    snapshot.playhead_samples = playhead_samples;
    snapshot.playhead_updated = Some(Instant::now());
    snapshot.playing = engine.transport().playing;
    if !snapshot.playing {
        snapshot.meters.clear();
    }
    snapshot.device_meters.clear();
    if snapshot.playing {
        for (device_id, meter) in engine.device_meter_snapshot() {
            snapshot.device_meters.insert(
                device_id,
                DeviceMeterSnapshot {
                    input_peak_l: meter.input_peak_l,
                    input_peak_r: meter.input_peak_r,
                    output_peak_l: meter.output_peak_l,
                    output_peak_r: meter.output_peak_r,
                },
            );
        }
    }
    snapshot.cpu_load = Some(dsp.usage_percent);
    snapshot.block_size = dsp.frames;
    snapshot.dsp.sample_rate = snapshot.sample_rate;

    snapshot.load_ms_history.push(dsp.load_ms);
    if snapshot.load_ms_history.len() > DSP_HISTORY {
        snapshot.load_ms_history.remove(0);
    }
    snapshot.jitter_history.push(dsp.jitter_percent);
    if snapshot.jitter_history.len() > DSP_HISTORY {
        snapshot.jitter_history.remove(0);
    }
    let load_avg =
        snapshot.load_ms_history.iter().sum::<f32>() / snapshot.load_ms_history.len().max(1) as f32;
    let load_max = snapshot
        .load_ms_history
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    let jitter_max = snapshot
        .jitter_history
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);

    snapshot.dsp.history.push(dsp.usage_percent);
    if snapshot.dsp.history.len() > DSP_HISTORY {
        snapshot.dsp.history.remove(0);
    }
    let rate = snapshot.dsp.sample_rate.max(1) as f32;
    snapshot.dsp.block_size = dsp.frames;
    snapshot.dsp.deadline_ms = dsp.frames as f32 / rate * 1000.0;
    snapshot.dsp.load_avg_ms = load_avg;
    snapshot.dsp.load_max_ms = load_max;
    snapshot.dsp.period_jitter_percent = jitter_max;
    snapshot.dsp.output_latency_samples = dsp.output_latency_samples;
    snapshot.dsp.input_latency_samples = 0;
    snapshot.dsp.timing = "Device".into();
    for event in events {
        match event {
            RtEvent::Meters {
                track_id,
                peak_l,
                peak_r,
                rms_l,
                rms_r,
            } => {
                snapshot.meters.insert(
                    *track_id,
                    MeterValues {
                        peak_l: *peak_l,
                        peak_r: *peak_r,
                        rms_l: *rms_l,
                        rms_r: *rms_r,
                    },
                );
            }
            RtEvent::Status { xruns, .. } => {
                snapshot.xruns = *xruns;
            }
            RtEvent::Error { message, .. } => {
                snapshot.error = Some(message.clone());
            }
            _ => {}
        }
    }
}
