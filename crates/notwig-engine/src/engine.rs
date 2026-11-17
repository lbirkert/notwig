use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use notwig_core::{AssetId, AudioAssetBuffer, EngineMsg, PlaybackSchedule, RtEvent, TimeMus};
use notwig_dsp::CompiledPlan;

use crate::factory::{DeviceFactory, StockDeviceFactory};
use crate::runtime::ActivePlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    pub sample_rate: u32,
    pub max_block_size: usize,
    pub channels: u16,
}

impl EngineConfig {
    #[must_use]
    pub fn new(sample_rate: u32, max_block_size: usize) -> Self {
        Self {
            sample_rate,
            max_block_size,
            channels: 2,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::new(48_000, 256)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine has no active plan")]
    NoPlan,
    #[error("engine has no playback schedule")]
    NoSchedule,
    #[error("invalid output buffer: expected {expected} frames, got {actual}")]
    InvalidBuffer { expected: usize, actual: usize },
    #[error("invalid plan: {reason}")]
    InvalidPlan { reason: String },
    #[error("graph contains a cycle")]
    Cycle,
    #[error("device preparation failed: {reason}")]
    Device { reason: String },
}

#[derive(Debug, Clone)]
pub struct TransportRuntime {
    pub playing: bool,
    pub playhead_samples: f64,
    pub loop_enabled: bool,
    pub loop_start_samples: u64,
    pub loop_end_samples: u64,
}

impl Default for TransportRuntime {
    fn default() -> Self {
        Self {
            playing: false,
            playhead_samples: 0.0,
            loop_enabled: false,
            loop_start_samples: 0,
            loop_end_samples: 0,
        }
    }
}

pub struct Engine {
    config: EngineConfig,
    plan: Option<Arc<CompiledPlan>>,
    schedule: Option<Arc<PlaybackSchedule>>,
    assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    transport: TransportRuntime,
    runtime: Option<ActivePlan>,
    events: VecDeque<RtEvent>,
    factory: Arc<dyn DeviceFactory>,
    xruns: u64,
    all_notes_off: bool,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("config", &self.config)
            .field("has_plan", &self.plan.is_some())
            .field("has_schedule", &self.schedule.is_some())
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

impl Engine {
    #[must_use]
    pub fn new(config: EngineConfig) -> Self {
        Self::with_factory(config, Arc::new(StockDeviceFactory))
    }

    #[must_use]
    pub fn with_factory(config: EngineConfig, factory: Arc<dyn DeviceFactory>) -> Self {
        Self {
            config,
            plan: None,
            schedule: None,
            assets: BTreeMap::new(),
            transport: TransportRuntime::default(),
            runtime: None,
            events: VecDeque::with_capacity(256),
            factory,
            xruns: 0,
            all_notes_off: false,
        }
    }

    #[must_use]
    pub fn config(&self) -> EngineConfig {
        self.config
    }

    #[must_use]
    pub fn playhead_samples(&self) -> u64 {
        self.transport.playhead_samples.max(0.0) as u64
    }

    #[must_use]
    pub fn transport(&self) -> &TransportRuntime {
        &self.transport
    }

    pub fn apply(&mut self, message: EngineMsg) -> Result<(), EngineError> {
        match message {
            EngineMsg::ApplyPlan { plan } => {
                let plan = plan.as_arc_any().downcast::<CompiledPlan>().map_err(|_| {
                    EngineError::InvalidPlan {
                        reason: "plan payload is not a CompiledPlan".into(),
                    }
                })?;
                self.plan = Some(plan);
                self.prepare()?;
            }
            EngineMsg::SetTransport {
                playing,
                playhead_samples,
                loop_enabled,
                loop_start_samples,
                loop_end_samples,
                stop,
            } => {
                let jumped = (self.transport.playhead_samples as u64) != playhead_samples;
                self.transport.playing = playing;
                self.transport.playhead_samples = playhead_samples as f64;
                self.transport.loop_enabled = loop_enabled;
                if let (Some(start), Some(end)) = (loop_start_samples, loop_end_samples) {
                    self.transport.loop_start_samples = start;
                    self.transport.loop_end_samples = end;
                }
                if stop || !playing || jumped {
                    self.all_notes_off = true;
                }
            }
            EngineMsg::SetParam {
                device_id,
                param_id,
                value,
                ..
            } => {
                if let Some(plan) = &mut self.plan {
                    let plan = Arc::make_mut(plan);
                    if let Some(device) = plan.devices.get_mut(&device_id) {
                        device.param_values.insert(param_id, value.clone());
                    }
                }
                if let Some(runtime) = &mut self.runtime {
                    runtime.set_param(device_id, param_id, &value);
                }
            }
            EngineMsg::SetTrackState {
                track_id,
                mute,
                solo_effective,
                gain_db,
                pan,
            } => {
                if let Some(plan) = &mut self.plan {
                    let plan = Arc::make_mut(plan);
                    if let Some(track) = plan.tracks.get_mut(&track_id) {
                        track.mute = mute;
                        track.solo_effective = solo_effective;
                        track.volume_db = gain_db;
                        track.pan = pan;
                    }
                }
                if let Some(runtime) = &mut self.runtime {
                    runtime.set_track_state(track_id, mute, solo_effective, gain_db, pan);
                }
            }
            EngineMsg::UpdateSchedule { schedule } => {
                self.schedule = Some(schedule);
                self.prepare()?;
            }
            EngineMsg::PrepareAudioAsset { asset_id, buffer } => {
                self.assets.insert(asset_id, buffer);
                self.prepare()?;
            }
            EngineMsg::AllNotesOff => {
                self.all_notes_off = true;
            }
        }
        Ok(())
    }

    fn prepare(&mut self) -> Result<(), EngineError> {
        let (Some(plan), Some(schedule)) = (&self.plan, &self.schedule) else {
            return Ok(());
        };
        let runtime = ActivePlan::build(
            plan,
            schedule,
            &self.assets,
            self.config,
            self.factory.as_ref(),
        )?;
        let latency = plan.total_latency;
        self.runtime = Some(runtime);
        self.push_event(RtEvent::LatencyChanged {
            device_id: None,
            samples: latency,
        });
        Ok(())
    }

    fn push_event(&mut self, event: RtEvent) {
        if self.events.len() >= 4096 {
            if matches!(event, RtEvent::Meters { .. }) {
                return;
            }
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn drain_events(&mut self, out: &mut Vec<RtEvent>) {
        out.extend(self.events.drain(..));
    }

    pub fn process(&mut self, output: &mut [f32], frames: usize) -> Result<(), EngineError> {
        let channels = usize::from(self.config.channels.max(2));
        if output.len() < frames * channels {
            return Err(EngineError::InvalidBuffer {
                expected: frames * channels,
                actual: output.len(),
            });
        }
        let stereo = &mut output[..frames * 2];
        stereo.fill(0.0);

        if self.runtime.is_none() {
            return Err(EngineError::NoPlan);
        }

        if !self.transport.playing {
            if let Some(runtime) = &mut self.runtime {
                runtime.emit_all_notes_off();
            }
            self.all_notes_off = false;
            self.transport_tick();
            return Ok(());
        }

        let mut written = 0usize;
        while written < frames {
            let remaining = frames - written;
            let mut segment = remaining;
            if self.transport.loop_enabled
                && self.transport.loop_end_samples > self.transport.loop_start_samples
            {
                let playhead = self.transport.playhead_samples.max(0.0) as u64;
                if playhead >= self.transport.loop_end_samples {
                    self.transport.playhead_samples = self.transport.loop_start_samples as f64;
                    self.all_notes_off = true;
                }
                let available = self
                    .transport
                    .loop_end_samples
                    .saturating_sub(self.transport.playhead_samples as u64)
                    as usize;
                segment = segment.min(available.max(1));
            }

            if self.all_notes_off {
                if let Some(runtime) = &mut self.runtime {
                    runtime.initiate_all_notes_off();
                }
                self.all_notes_off = false;
            }

            let start = self.transport.playhead_samples.max(0.0) as u64;
            let render_result = if let Some(runtime) = &mut self.runtime {
                runtime.render(
                    start,
                    segment,
                    &mut stereo[written * 2..(written + segment) * 2],
                    self.transport.playing,
                )
            } else {
                Err(EngineError::NoPlan)
            };
            if let Err(err) = render_result {
                self.xruns = self.xruns.saturating_add(1);
                self.push_event(RtEvent::Error {
                    scope: "engine".into(),
                    code: "render".into(),
                    message: err.to_string(),
                });
                for sample in &mut stereo[written * 2..(written + segment) * 2] {
                    *sample = 0.0;
                }
            }

            written += segment;
            self.transport.playhead_samples += segment as f64;
            if self.transport.loop_enabled
                && self.transport.playhead_samples as u64 >= self.transport.loop_end_samples
            {
                self.transport.playhead_samples = self.transport.loop_start_samples as f64;
                self.all_notes_off = true;
            }
        }

        self.push_event(RtEvent::Status {
            xruns: self.xruns,
            cpu_load: None,
        });
        let meters = self
            .runtime
            .as_ref()
            .map(notwig_engine_meter_snapshot)
            .unwrap_or_default();
        for (track_id, peak_l, peak_r, rms_l, rms_r) in meters {
            self.push_event(RtEvent::Meters {
                track_id,
                peak_l,
                peak_r,
                rms_l,
                rms_r,
            });
        }
        self.transport_tick();
        Ok(())
    }

    fn transport_tick(&mut self) {
        let samples = self.playhead_samples();
        let playhead_mus = self.plan.as_ref().map_or(TimeMus::zero(), |plan| {
            samples_to_mus(samples, plan.config.tempo_bpm, plan.config.sample_rate)
        });
        self.push_event(RtEvent::TransportTick {
            playhead_samples: samples,
            playhead_mus,
            playing: self.transport.playing,
        });
    }

    pub fn render_offline(
        &mut self,
        start_sample: u64,
        end_sample: u64,
    ) -> Result<Vec<f32>, EngineError> {
        if end_sample <= start_sample {
            return Ok(Vec::new());
        }
        self.transport.playing = true;
        self.transport.playhead_samples = start_sample as f64;
        self.transport.loop_enabled = false;
        self.all_notes_off = true;
        if let Some(runtime) = &mut self.runtime {
            runtime.reset();
        }
        let total = usize::try_from(end_sample - start_sample).unwrap_or(0);
        let mut output = vec![0.0; total * 2];
        let block = self.config.max_block_size.max(1);
        let mut offset = 0usize;
        while offset < total {
            let frames = block.min(total - offset);
            self.process(&mut output[offset * 2..(offset + frames) * 2], frames)?;
            offset += frames;
        }
        Ok(output)
    }
}

fn samples_to_mus(samples: u64, tempo_bpm: f64, sample_rate: u32) -> TimeMus {
    let samples_per_beat = f64::from(sample_rate) * 60.0 / tempo_bpm;
    if samples_per_beat <= 0.0 {
        return TimeMus::zero();
    }
    let beats = samples as f64 / samples_per_beat;
    TimeMus::new((beats * 1_000_000.0).round() as i64, 1_000_000)
}

fn notwig_engine_meter_snapshot(
    plan: &ActivePlan,
) -> Vec<(notwig_core::TrackId, f32, f32, f32, f32)> {
    plan.meter_snapshot()
}
