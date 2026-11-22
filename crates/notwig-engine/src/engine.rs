use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use notwig_core::{AssetId, AudioAssetBuffer, EngineMsg, PlaybackSchedule, RtEvent, TimeMus};
use notwig_dsp::{CompiledPlan, DeviceFactory, StockDeviceFactory};

use crate::control::{ControlHandle, EngineData, PreparedRuntime};
use crate::runtime::ActivePlan;

/// The audio thread checks the shared control slot once per this many frames.
/// This is deterministic and keeps the lock attempt off most callbacks.
pub const DEFAULT_CONTROL_SYNC_FRAMES: u64 = 256;

/// Final hard ceiling for the stereo output, +10 dBFS (`10^(10/20)`).
///
/// A misbehaving plugin can otherwise emit extreme sample values when it is
/// enabled or disabled. Clipping at the very end of the engine keeps those
/// spikes away from speakers while leaving normal material untouched.
const SAFETY_CLIP_CEILING: f32 = 3.162_277_7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    transport: TransportRuntime,
    runtime: Option<ActivePlan>,
    events: VecDeque<RtEvent>,
    factory: Arc<dyn DeviceFactory>,
    xruns: u64,
    all_notes_off: bool,
    control: Option<ControlHandle>,
    control_data: Box<EngineData>,
    control_sync_frames: u64,
    control_frames_until_sync: u64,
    metronome_enabled: bool,
    metronome_beats_per_bar: u8,
    offline: bool,
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
            assets: Arc::new(BTreeMap::new()),
            transport: TransportRuntime::default(),
            runtime: None,
            events: VecDeque::with_capacity(256),
            factory,
            xruns: 0,
            all_notes_off: false,
            control: None,
            control_data: Box::default(),
            control_sync_frames: DEFAULT_CONTROL_SYNC_FRAMES,
            control_frames_until_sync: DEFAULT_CONTROL_SYNC_FRAMES,
            metronome_enabled: false,
            metronome_beats_per_bar: 4,
            offline: false,
        }
    }

    /// Attach the app-side control slot. The audio thread pulls updates from
    /// this handle at a fixed frame interval without blocking.
    pub fn attach_control(&mut self, control: ControlHandle) {
        self.control = Some(control);
        self.control_frames_until_sync = self.control_sync_frames.max(1);
    }

    pub fn set_control_sync_frames(&mut self, frames: u64) {
        self.control_sync_frames = frames.max(1);
        self.control_frames_until_sync =
            self.control_frames_until_sync.min(self.control_sync_frames);
    }

    /// Build a runtime on a loader/build thread. The audio thread only swaps
    /// this prepared value into place.
    pub fn prepare_runtime(
        config: EngineConfig,
        factory: Arc<dyn DeviceFactory>,
        plan: Arc<CompiledPlan>,
        schedule: Arc<PlaybackSchedule>,
        assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    ) -> Result<Box<PreparedRuntime>, EngineError> {
        let runtime = ActivePlan::build(
            &plan,
            &schedule,
            Arc::clone(&assets),
            config,
            factory.as_ref(),
        )?;
        Ok(Box::new(PreparedRuntime {
            plan,
            schedule,
            assets,
            runtime,
        }))
    }

    /// Construct an offline/test engine from a prepared runtime without
    /// rebuilding processors.
    #[must_use]
    pub fn from_prepared(
        config: EngineConfig,
        factory: Arc<dyn DeviceFactory>,
        prepared: Box<PreparedRuntime>,
        transport: TransportRuntime,
        all_notes_off: bool,
    ) -> Self {
        let PreparedRuntime {
            plan,
            schedule,
            assets,
            runtime,
        } = *prepared;
        let mut engine = Self::with_factory(config, factory);
        engine.plan = Some(plan);
        engine.schedule = Some(schedule);
        engine.assets = assets;
        engine.runtime = Some(runtime);
        engine.transport = transport;
        engine.all_notes_off = all_notes_off;
        engine
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

    /// Returns the latest per-device input/output peaks.
    #[must_use]
    pub fn device_meter_snapshot(
        &self,
    ) -> Vec<(notwig_core::DeviceId, crate::runtime::DeviceMeterRuntime)> {
        self.runtime
            .as_ref()
            .map(ActivePlan::device_meter_snapshot)
            .unwrap_or_default()
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
                metronome,
            } => {
                let jumped = (self.transport.playhead_samples as u64) != playhead_samples;
                self.transport.playing = playing;
                self.transport.playhead_samples = playhead_samples as f64;
                self.transport.loop_enabled = loop_enabled;
                if let (Some(start), Some(end)) = (loop_start_samples, loop_end_samples) {
                    self.transport.loop_start_samples = start;
                    self.transport.loop_end_samples = end;
                }
                if let Some(metronome) = metronome {
                    self.metronome_enabled = metronome.enabled;
                    self.metronome_beats_per_bar = metronome.beats_per_bar.max(1);
                }
                if stop || !playing || jumped {
                    self.all_notes_off = true;
                }
                if jumped && let Some(runtime) = &mut self.runtime {
                    runtime.prime_notes_at(playhead_samples);
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
                self.schedule = Some(Arc::clone(&schedule));
                if let Some(runtime) = &mut self.runtime {
                    runtime.set_schedule(schedule);
                } else {
                    self.prepare()?;
                }
            }
            EngineMsg::PrepareAudioAsset { asset_id, buffer } => {
                Arc::make_mut(&mut self.assets).insert(asset_id, Arc::clone(&buffer));
                if let Some(runtime) = &mut self.runtime {
                    runtime.set_asset(asset_id, buffer);
                } else {
                    self.prepare()?;
                }
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
            Arc::clone(&self.assets),
            self.config,
            self.factory.as_ref(),
        )?;
        let latency = plan.total_latency;
        let playhead = self.playhead_samples();
        self.runtime = Some(runtime);
        if let Some(runtime) = &mut self.runtime {
            runtime.prime_notes_at(playhead);
        }
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

    fn maybe_sync_control(&mut self, frames: usize) {
        let Some(control) = &self.control else {
            return;
        };
        self.control_frames_until_sync =
            self.control_frames_until_sync.saturating_sub(frames as u64);
        if self.control_frames_until_sync > 0 {
            return;
        }
        self.control_frames_until_sync = self.control_sync_frames.max(1);

        // Never block the callback. If the app is publishing, skip this sync
        // point and retry at the next interval.
        let Ok(mut exchange) = control.exchange().try_lock() else {
            return;
        };
        if !exchange.updated {
            return;
        }
        let Some(incoming) = exchange.data.take() else {
            exchange.updated = false;
            return;
        };
        let retired = std::mem::replace(&mut self.control_data, incoming);
        exchange.data = Some(retired);
        exchange.updated = false;
        drop(exchange);

        self.apply_control_data();
    }

    fn apply_control_data(&mut self) {
        if let Some(prepared) = self.control_data.prepared.take() {
            let PreparedRuntime {
                plan,
                schedule,
                assets,
                runtime,
            } = *prepared;
            if let Some(old) = self.runtime.replace(runtime) {
                self.control_data.retired_runtime = Some(old);
            }
            if let Some(old) = self.plan.replace(plan) {
                self.control_data.retired_plan = Some(old);
            }
            if let Some(old) = self.schedule.replace(schedule) {
                self.control_data.retired_schedule.push(old);
            }
            let old_assets = std::mem::replace(&mut self.assets, assets);
            self.control_data.retired_assets.push(old_assets);

            let latency = self.plan.as_ref().map_or(0, |plan| plan.total_latency);
            self.push_event(RtEvent::LatencyChanged {
                device_id: None,
                samples: latency,
            });
            // The new runtime has no note state yet. Re-prime whatever is
            // sounding under the current playhead so a structural rebuild does
            // not silence sustained MIDI.
            if let Some(runtime) = &mut self.runtime {
                runtime.prime_notes_at(self.transport.playhead_samples.max(0.0) as u64);
            }
        }

        if let Some(assets) = self.control_data.assets.take() {
            let old_assets = std::mem::replace(&mut self.assets, Arc::clone(&assets));
            self.control_data.retired_assets.push(old_assets);
            if let Some(runtime) = &mut self.runtime {
                let old_runtime_assets = runtime.replace_assets(assets);
                self.control_data.retired_assets.push(old_runtime_assets);
            }
        }

        if let Some(schedule) = &self.control_data.schedule
            && !self
                .schedule
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, schedule))
        {
            let schedule = Arc::clone(schedule);
            if let Some(old) = self.schedule.replace(Arc::clone(&schedule)) {
                self.control_data.retired_schedule.push(old);
            }
            if let Some(runtime) = &mut self.runtime {
                runtime.set_schedule(schedule);
            }
        }

        if let Some(transport) = self.control_data.transport {
            let jumped = (self.transport.playhead_samples as u64) != transport.playhead_samples;
            self.transport.playing = transport.playing;
            self.transport.playhead_samples = transport.playhead_samples as f64;
            self.transport.loop_enabled = transport.loop_enabled;
            if let (Some(start), Some(end)) =
                (transport.loop_start_samples, transport.loop_end_samples)
            {
                self.transport.loop_start_samples = start;
                self.transport.loop_end_samples = end;
            }
            if let Some(metronome) = transport.metronome {
                self.metronome_enabled = metronome.enabled;
                self.metronome_beats_per_bar = metronome.beats_per_bar.max(1);
            }
            if transport.stop || !transport.playing || jumped {
                self.all_notes_off = true;
            }
            if jumped && let Some(runtime) = &mut self.runtime {
                runtime.prime_notes_at(transport.playhead_samples);
            }
        }

        if let Some(metronome) = self.control_data.metronome {
            self.metronome_enabled = metronome.enabled;
            self.metronome_beats_per_bar = metronome.beats_per_bar.max(1);
        }

        if let Some(runtime) = &mut self.runtime {
            for update in &self.control_data.track_states {
                runtime.set_track_state(
                    update.track_id,
                    update.mute,
                    update.solo_effective,
                    update.gain_db,
                    update.pan,
                );
            }
            for update in &self.control_data.params {
                runtime.set_param(update.device_id, update.param_id, &update.value);
            }
            for preview in self.control_data.preview_notes.drain(..) {
                runtime.queue_preview_note(preview);
            }
            for (track_id, note_id) in self.control_data.preview_offs.drain(..) {
                runtime.release_preview_note(track_id, note_id);
            }
        }

        if self.control_data.all_notes_off {
            self.all_notes_off = true;
        }
    }

    pub fn process(&mut self, output: &mut [f32], frames: usize) -> Result<(), EngineError> {
        let channels = usize::from(self.config.channels.max(2));
        if output.len() < frames * channels {
            return Err(EngineError::InvalidBuffer {
                expected: frames * channels,
                actual: output.len(),
            });
        }
        self.maybe_sync_control(frames);
        if self.runtime.is_none() {
            return Err(EngineError::NoPlan);
        }
        let chunk = self.config.max_block_size.max(1);
        let mut written = 0;
        while written < frames {
            let count = (frames - written).min(chunk);
            let range = written * 2..(written + count) * 2;
            self.process_chunk(&mut output[range], count)?;
            written += count;
        }
        // Clip the final interleaved output after every chunk has been mixed.
        // Non-finite plugin output is replaced with silence rather than passed
        // through to the device.
        for sample in &mut output[..frames * channels] {
            *sample = if sample.is_finite() {
                sample.clamp(-SAFETY_CLIP_CEILING, SAFETY_CLIP_CEILING)
            } else {
                0.0
            };
        }
        if let Some(control) = &self.control {
            control.store_playhead_samples(self.playhead_samples());
        }
        Ok(())
    }

    fn process_chunk(&mut self, stereo: &mut [f32], frames: usize) -> Result<(), EngineError> {
        stereo.fill(0.0);
        if self.runtime.is_none() {
            return Err(EngineError::NoPlan);
        }

        if !self.transport.playing {
            if let Some(runtime) = &mut self.runtime {
                // A stopped transport silences active notes once, but must keep
                // rendering so UI-triggered preview notes can still sound.
                if self.all_notes_off {
                    runtime.initiate_all_notes_off();
                    self.all_notes_off = false;
                }
                let start = self.transport.playhead_samples.max(0.0) as u64;
                let _ = runtime.render(start, frames, stereo, false, false);
            }
            if !self.offline {
                self.transport_tick();
            }
            return Ok(());
        }

        let mut written = 0usize;
        while written < frames {
            let remaining = frames - written;
            let mut segment = remaining;
            let loop_active = self.transport.loop_enabled
                && self.transport.loop_end_samples > self.transport.loop_start_samples;
            let playhead_before = self.transport.playhead_samples.max(0.0) as u64;
            // Only wrap when the playhead actually crosses the loop end. A
            // playhead already at or beyond the end keeps playing forward; it
            // should not be yanked back to the loop start.
            let crosses_loop_end = loop_active && playhead_before < self.transport.loop_end_samples;
            if crosses_loop_end {
                let available = self.transport.loop_end_samples - playhead_before;
                segment = segment.min(usize::try_from(available).unwrap_or(usize::MAX));
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
                    true,
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
            } else {
                self.mix_metronome(
                    &mut stereo[written * 2..(written + segment) * 2],
                    start,
                    segment,
                );
            }

            written += segment;
            self.transport.playhead_samples += segment as f64;
            if crosses_loop_end
                && self.transport.playhead_samples as u64 >= self.transport.loop_end_samples
            {
                self.transport.playhead_samples = self.transport.loop_start_samples as f64;
                self.all_notes_off = true;
            }
        }

        if self.offline {
            return Ok(());
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

    fn mix_metronome(&self, buffer: &mut [f32], start_sample: u64, frames: usize) {
        if !self.metronome_enabled {
            return;
        }
        let Some(plan) = &self.plan else {
            return;
        };
        let rate = f64::from(self.config.sample_rate.max(1));
        let samples_per_beat = rate * 60.0 / plan.config.tempo_bpm.max(1e-6);
        if samples_per_beat <= 0.0 {
            return;
        }
        let click_len = (0.03 * rate).max(1.0).min(samples_per_beat);
        let beats_per_bar = f64::from(self.metronome_beats_per_bar.max(1));
        for index in 0..frames {
            let absolute = start_sample as f64 + index as f64;
            let beat_position = absolute % samples_per_beat;
            if beat_position >= click_len {
                continue;
            }
            let beat_index = (absolute / samples_per_beat).floor();
            let accent = (beat_index % beats_per_bar).abs() < 1e-9;
            let frequency = if accent { 1760.0 } else { 880.0 };
            let envelope = (1.0 - beat_position / click_len).powi(2);
            let amplitude = if accent { 0.35 } else { 0.22 };
            let seconds = beat_position / rate;
            let sample = (seconds * frequency * std::f64::consts::TAU).sin() as f32
                * envelope as f32
                * amplitude;
            buffer[index * 2] += sample;
            buffer[index * 2 + 1] += sample;
        }
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

    pub fn begin_offline_render(&mut self, start_sample: u64) {
        self.transport.playing = true;
        self.transport.playhead_samples = start_sample as f64;
        self.transport.loop_enabled = false;
        self.all_notes_off = true;
        self.offline = true;
        self.events.clear();
        if let Some(runtime) = &mut self.runtime {
            runtime.reset();
            runtime.set_measure_meters(false);
            runtime.prime_notes_at(start_sample);
        }
    }

    pub fn render_offline_block(&mut self, frames: usize) -> Result<Vec<f32>, EngineError> {
        let frames = frames.clamp(1, self.config.max_block_size.max(1));
        let mut output = vec![0.0; frames * 2];
        self.process(&mut output, frames)?;
        Ok(output)
    }

    pub fn render_offline(
        &mut self,
        start_sample: u64,
        end_sample: u64,
    ) -> Result<Vec<f32>, EngineError> {
        if end_sample <= start_sample {
            return Ok(Vec::new());
        }
        self.begin_offline_render(start_sample);
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
