use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use notwig_core::dsp::{
    CompiledPlan, DeviceProcessor, PlanTrack, ProcessContext, RenderConfig, TransportInfo,
};
use notwig_core::{
    AssetId, AudioAssetBuffer, Db, DeviceChainId, DeviceId, NoteId, Pan, ParamId, ParamValue,
    PlaybackSchedule, SendId, TrackId,
};

use crate::control::PreviewNote;
use crate::engine::{EngineConfig, EngineError};
use crate::input::{InputBus, MAX_INPUT_TRACKS};
use notwig_core::dsp::DeviceFactory;

const PREVIEW_POOL_CAPACITY: usize = 32;

/// Upper bound on per-block device faults recorded without allocating. A block
/// with more failing devices than this keeps the first entries and skips the
/// rest; the cap only matters when many devices fail at once.
const MAX_RUNTIME_FAULTS: usize = 32;

mod clips;
mod events;
mod graph;
mod low_latency;
mod routing;

use low_latency::LowLatencyMonitorRuntime;
use routing::stereo_peaks;

#[derive(Debug, Clone, Copy)]
struct PreviewSlot {
    track_id: TrackId,
    note_id: NoteId,
    pitch: u8,
    velocity: f32,
    on_emitted: bool,
    frames_until_off: u32,
}

#[derive(Debug)]
struct DelayLine {
    buffer: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    fn new(samples: u32) -> Self {
        Self {
            buffer: vec![0.0; samples as usize * 2],
            pos: 0,
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32], frames: usize) {
        let len = self.buffer.len() / 2;
        if len == 0 {
            output[..frames * 2].copy_from_slice(&input[..frames * 2]);
            return;
        }
        let mut pos = self.pos % len;
        for frame in 0..frames {
            for channel in 0..2 {
                let index = pos * 2 + channel;
                output[frame * 2 + channel] = self.buffer[index];
                self.buffer[index] = input[frame * 2 + channel];
            }
            pos = (pos + 1) % len;
        }
        self.pos = pos;
    }

    fn process_in_place(&mut self, audio: &mut [f32], frames: usize) {
        let len = self.buffer.len() / 2;
        if len == 0 {
            return;
        }
        let mut pos = self.pos % len;
        for frame in 0..frames {
            for channel in 0..2 {
                let index = pos * 2 + channel;
                std::mem::swap(&mut audio[frame * 2 + channel], &mut self.buffer[index]);
            }
            pos = (pos + 1) % len;
        }
        self.pos = pos;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }
}

/// Input and output peaks measured around one device in the processing chain.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeviceMeterRuntime {
    pub input_peak_l: f32,
    pub input_peak_r: f32,
    pub output_peak_l: f32,
    pub output_peak_r: f32,
}

/// A device that reported a failed/overloaded block during the last render.
///
/// The engine drains these after each block and turns them into
/// [`notwig_core::RtEvent::Error`] so a misbehaving processor is observable in
/// the audio snapshot instead of being silently ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeFault {
    pub device_id: DeviceId,
}

/// A device in a low-latency monitor chain that could not be prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LowLatencyMonitorFailure {
    pub track_id: TrackId,
    pub device_id: DeviceId,
    pub reason: String,
}

#[derive(Debug)]
pub struct TrackRuntime {
    pub plan: PlanTrack,
    pub monitor: notwig_core::MonitorMode,
    pub buffer: Vec<f32>,
    pub pre_fader: Vec<f32>,
    input_delay: DelayLine,
    main_delay: DelayLine,
    send_delays: BTreeMap<SendId, DelayLine>,
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

impl TrackRuntime {
    fn new(plan: PlanTrack, config: RenderConfig, main_delay_samples: u32) -> Self {
        let frames = config.max_block_size.max(1);
        let send_delays = plan
            .sends
            .iter()
            .map(|send| (send.id, DelayLine::new(send.latency_samples)))
            .collect();
        let monitor = plan.monitor;
        Self {
            input_delay: DelayLine::new(plan.input_latency),
            main_delay: DelayLine::new(main_delay_samples),
            send_delays,
            plan,
            monitor,
            buffer: vec![0.0; frames * 2],
            pre_fader: vec![0.0; frames * 2],
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pre_fader.fill(0.0);
        self.input_delay.reset();
        self.main_delay.reset();
        for delay in self.send_delays.values_mut() {
            delay.reset();
        }
        self.peak_l = 0.0;
        self.peak_r = 0.0;
        self.rms_l = 0.0;
        self.rms_r = 0.0;
    }

    pub(super) fn monitor_gain(&self, apply_pan: bool) -> (f32, f32) {
        let audible = self.plan.solo_effective && !self.plan.mute;
        let gain = if audible {
            self.plan.volume_db.to_gain().0
        } else {
            0.0
        };
        let (pan_l, pan_r) = if apply_pan {
            self.plan.pan.gains()
        } else {
            (1.0, 1.0)
        };
        (gain * pan_l, gain * pan_r)
    }
}

pub struct ActivePlan {
    config: RenderConfig,
    master: TrackId,
    order: Vec<TrackId>,
    tracks: BTreeMap<TrackId, TrackRuntime>,
    chains: BTreeMap<DeviceChainId, Vec<DeviceId>>,
    processors: Vec<Option<Box<dyn DeviceProcessor>>>,
    processor_index: BTreeMap<DeviceId, usize>,
    low_monitors: BTreeMap<TrackId, LowLatencyMonitorRuntime>,
    device_meters: BTreeMap<DeviceId, DeviceMeterRuntime>,
    schedule: Arc<PlaybackSchedule>,
    assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    active_notes: BTreeMap<TrackId, BTreeMap<NoteId, (u8, f32)>>,
    preview_pool: [Option<PreviewSlot>; PREVIEW_POOL_CAPACITY],
    panic_pending: bool,
    prime_at_sample: Option<u64>,
    tempo_bpm: f64,
    sample_rate: u32,
    route_scratch: Vec<f32>,
    runtime_faults: Vec<RuntimeFault>,
    low_monitor_failures: Vec<LowLatencyMonitorFailure>,
    low_latency_monitoring: bool,
    measure_meters: bool,
}

impl ActivePlan {
    pub(crate) fn build(
        plan: &Arc<CompiledPlan>,
        schedule: &Arc<PlaybackSchedule>,
        assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
        engine_config: EngineConfig,
        factory: &dyn DeviceFactory,
    ) -> Result<Self, EngineError> {
        let config = RenderConfig {
            sample_rate: plan.config.sample_rate,
            max_block_size: engine_config.max_block_size.max(1),
            channels: engine_config.channels.max(2),
            tempo_bpm: plan.config.tempo_bpm,
        };
        let order = graph::processing_order(plan)?;
        let mut tracks = BTreeMap::new();
        for (id, track) in &plan.tracks {
            let main_delay = graph::output_target(track, plan.master)
                .and_then(|target| plan.tracks.get(&target))
                .map_or(0, |target| {
                    target.input_latency.saturating_sub(track.output_latency)
                });
            tracks.insert(*id, TrackRuntime::new(track.clone(), config, main_delay));
        }
        let mut processors = Vec::new();
        let mut processor_index = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for track in plan.tracks.values().filter(|track| track.active) {
            if let Some(chain) = plan.chains.get(&track.main_chain) {
                roots.extend(chain.iter().copied());
            }
        }
        for device_id in roots {
            if let Some(mut processor) = graph::build_processor(plan, device_id, factory) {
                processor
                    .prepare(config)
                    .map_err(|err| EngineError::Device {
                        reason: err.to_string(),
                    })?;
                processor_index.insert(device_id, processors.len());
                processors.push(Some(processor));
            }
        }
        let mut chains = BTreeMap::new();
        for (chain, devices) in &plan.chains {
            chains.insert(*chain, devices.clone());
        }
        let mut low_monitors = BTreeMap::new();
        let mut low_monitor_failures = Vec::new();
        for (id, track) in &plan.tracks {
            if track.monitor != notwig_core::MonitorMode::LowLatency {
                continue;
            }
            let Some(chain) = plan.chains.get(&track.main_chain) else {
                continue;
            };
            let mut roots = Vec::new();
            for device_id in chain {
                if let Some(mut processor) = graph::build_processor(plan, *device_id, factory) {
                    match processor.prepare(config) {
                        Ok(()) => roots.push((*device_id, processor)),
                        Err(err) => low_monitor_failures.push(LowLatencyMonitorFailure {
                            track_id: *id,
                            device_id: *device_id,
                            reason: err.to_string(),
                        }),
                    }
                }
            }
            low_monitors.insert(
                *id,
                LowLatencyMonitorRuntime::new(roots, config.max_block_size),
            );
        }
        Ok(Self {
            config,
            master: plan.master,
            order,
            tracks,
            chains,
            processors,
            processor_index,
            low_monitors,
            device_meters: BTreeMap::new(),
            schedule: Arc::clone(schedule),
            assets,
            active_notes: BTreeMap::new(),
            preview_pool: [None; PREVIEW_POOL_CAPACITY],
            panic_pending: false,
            prime_at_sample: None,
            tempo_bpm: plan.config.tempo_bpm,
            sample_rate: plan.config.sample_rate,
            route_scratch: vec![0.0; config.max_block_size.max(1) * 2],
            runtime_faults: Vec::with_capacity(MAX_RUNTIME_FAULTS),
            low_monitor_failures,
            low_latency_monitoring: true,
            measure_meters: true,
        })
    }

    pub fn set_measure_meters(&mut self, measure: bool) {
        self.measure_meters = measure;
    }

    pub fn set_low_latency_monitoring(&mut self, enabled: bool) {
        self.low_latency_monitoring = enabled;
    }

    pub fn reset(&mut self) {
        self.active_notes.clear();
        self.preview_pool = [None; PREVIEW_POOL_CAPACITY];
        self.panic_pending = false;
        self.prime_at_sample = None;
        for track in self.tracks.values_mut() {
            track.reset();
        }
        for processor in self.processors.iter_mut().flatten() {
            processor.reset();
        }
        for monitor in self.low_monitors.values_mut() {
            monitor.reset();
        }
        self.device_meters.clear();
    }

    pub(crate) fn set_schedule(&mut self, schedule: Arc<PlaybackSchedule>) {
        self.schedule = schedule;
    }

    pub(crate) fn set_asset(&mut self, asset_id: AssetId, buffer: Arc<AudioAssetBuffer>) {
        Arc::make_mut(&mut self.assets).insert(asset_id, buffer);
    }

    pub(crate) fn replace_assets(
        &mut self,
        assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    ) -> Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>> {
        std::mem::replace(&mut self.assets, assets)
    }

    pub(crate) fn queue_preview_note(&mut self, preview: PreviewNote) {
        let Some(slot) = self.preview_pool.iter_mut().find(|slot| slot.is_none()) else {
            return;
        };
        *slot = Some(PreviewSlot {
            track_id: preview.track_id,
            note_id: preview.note_id,
            pitch: preview.pitch,
            velocity: preview.velocity.clamp(0.0, 1.0),
            on_emitted: false,
            frames_until_off: preview.length_mus.map_or(u32::MAX, |length| {
                let map = notwig_core::TempoMap {
                    tempo_bpm: self.tempo_bpm,
                    sample_rate: notwig_core::SampleRate(self.sample_rate),
                };
                u32::try_from(map.mus_to_samples(length).max(1))
                    .unwrap_or(u32::MAX)
                    .max(1)
            }),
        });
    }

    pub(crate) fn release_preview_note(&mut self, track_id: TrackId, note_id: NoteId) {
        for preview in self.preview_pool.iter_mut().flatten() {
            if preview.track_id == track_id && preview.note_id == note_id {
                preview.frames_until_off = 0;
            }
        }
    }

    pub fn initiate_all_notes_off(&mut self) {
        self.panic_pending = true;
    }

    /// Re-emit note-ons for notes that are already sounding at `start_sample`.
    ///
    /// A freshly built runtime has empty note state, and a seek moves the
    /// playhead away from the note-on that originally started a sustained note.
    /// The next rendered block emits these note-ons at offset zero so held notes
    /// do not disappear.
    pub(crate) fn prime_notes_at(&mut self, start_sample: u64) {
        self.prime_at_sample = Some(start_sample);
    }

    pub fn emit_all_notes_off(&mut self) {
        self.active_notes.clear();
        self.panic_pending = false;
    }

    pub fn set_param(&mut self, device_id: DeviceId, param_id: ParamId, value: &ParamValue) {
        if let Some(index) = self.processor_index.get(&device_id).copied() {
            if let Some(processor) = self.processors[index].as_mut() {
                processor.set_param(param_id, value);
            }
        } else {
            for processor in self.processors.iter_mut().flatten() {
                if processor.set_nested_param(device_id, param_id, value) {
                    break;
                }
            }
        }
        for monitor in self.low_monitors.values_mut() {
            monitor.set_param(device_id, param_id, value);
        }
    }

    pub fn set_track_state(
        &mut self,
        track_id: TrackId,
        mute: bool,
        solo_effective: bool,
        gain_db: Db,
        pan: Pan,
    ) {
        if let Some(track) = self.tracks.get_mut(&track_id) {
            track.plan.mute = mute;
            track.plan.solo_effective = solo_effective;
            track.plan.volume_db = gain_db;
            track.plan.pan = pan;
        }
    }

    pub fn set_track_monitor(&mut self, track_id: TrackId, monitor: notwig_core::MonitorMode) {
        if let Some(track) = self.tracks.get_mut(&track_id) {
            track.monitor = monitor;
        }
    }

    pub fn meter_snapshot(&self) -> Vec<(TrackId, f32, f32, f32, f32)> {
        let mut out = Vec::with_capacity(self.tracks.len());
        self.meter_snapshot_into(&mut out);
        out
    }

    /// Copy the current meter values into `out` without allocating.
    ///
    /// `out` keeps its capacity across blocks so the audio thread stays out of
    /// the allocator when it publishes meters.
    pub fn meter_snapshot_into(&self, out: &mut Vec<(TrackId, f32, f32, f32, f32)>) {
        out.clear();
        out.extend(
            self.tracks
                .iter()
                .map(|(id, track)| (*id, track.peak_l, track.peak_r, track.rms_l, track.rms_r)),
        );
    }

    /// Append the device faults recorded during the last block into `out`.
    ///
    /// This swaps out the runtime's queue so the next block starts clean.
    pub(crate) fn take_faults_into(&mut self, out: &mut Vec<RuntimeFault>) {
        out.clear();
        out.append(&mut self.runtime_faults);
    }

    /// Take the low-latency monitor prepare failures recorded at build time.
    pub(crate) fn take_low_monitor_failures(&mut self) -> Vec<LowLatencyMonitorFailure> {
        std::mem::take(&mut self.low_monitor_failures)
    }

    pub fn device_meter_snapshot(&self) -> Vec<(DeviceId, DeviceMeterRuntime)> {
        self.device_meters
            .iter()
            .map(|(id, meter)| (*id, *meter))
            .collect()
    }

    pub fn render(
        &mut self,
        start_sample: u64,
        frames: usize,
        output: &mut [f32],
        playing: bool,
        include_clips: bool,
        input_bus: Option<&InputBus>,
        input_routes: &[Option<(TrackId, u16)>; MAX_INPUT_TRACKS],
    ) -> Result<(), EngineError> {
        let max_frames = self.config.max_block_size.max(1);
        if frames > max_frames {
            let mut written = 0;
            while written < frames {
                let count = (frames - written).min(max_frames);
                let range = written * 2..(written + count) * 2;
                self.render_inner(
                    start_sample.saturating_add(written as u64),
                    count,
                    &mut output[range],
                    playing,
                    include_clips,
                    input_bus,
                    input_routes,
                );
                written += count;
            }
            return Ok(());
        }
        self.render_inner(
            start_sample,
            frames,
            output,
            playing,
            include_clips,
            input_bus,
            input_routes,
        );
        Ok(())
    }

    fn render_inner(
        &mut self,
        start_sample: u64,
        frames: usize,
        output: &mut [f32],
        playing: bool,
        include_clips: bool,
        input_bus: Option<&InputBus>,
        input_routes: &[Option<(TrackId, u16)>; MAX_INPUT_TRACKS],
    ) {
        self.runtime_faults.clear();
        for track in self.tracks.values_mut() {
            track.buffer[..frames * 2].fill(0.0);
            track.pre_fader[..frames * 2].fill(0.0);
        }

        if let Some(bus) = input_bus {
            for (track_id, slot) in input_routes.iter().flatten() {
                let mode = self
                    .tracks
                    .get(track_id)
                    .map_or(notwig_core::MonitorMode::Enabled, |track| track.monitor);
                let mode = if mode == notwig_core::MonitorMode::LowLatency
                    && !self.low_latency_monitoring
                {
                    notwig_core::MonitorMode::Enabled
                } else {
                    mode
                };
                match mode {
                    notwig_core::MonitorMode::Disabled => bus.discard_monitor(*slot, frames),
                    notwig_core::MonitorMode::Enabled => {
                        if let Some(track) = self.tracks.get_mut(track_id) {
                            bus.drain_monitor_into(*slot, frames, &mut track.buffer[..frames * 2]);
                        }
                    }
                    notwig_core::MonitorMode::LowLatency => {
                        // The low-latency monitor path drains this slot in the
                        // device callback so it stays outside the high-latency
                        // project chunk.
                    }
                }
            }
        }

        if include_clips {
            self.render_audio_clips(start_sample, frames);
        }
        for track in self.tracks.values_mut() {
            track
                .input_delay
                .process_in_place(&mut track.buffer, frames);
        }
        let mut events = self.collect_events(start_sample, frames, include_clips);

        let map = notwig_core::TempoMap {
            tempo_bpm: self.tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
        let transport = TransportInfo {
            playhead_mus: map.samples_to_mus(start_sample),
            loop_start_mus: None,
            loop_end_mus: None,
        };
        let ctx = ProcessContext {
            config: self.config,
            sample_position: start_sample,
            playing,
            tempo_bpm: self.tempo_bpm,
            transport: &transport,
        };

        for index in 0..self.order.len() {
            let id = self.order[index];
            let Some(chain) = self.tracks.get(&id).map(|track| track.plan.main_chain) else {
                continue;
            };
            let device_count = self.chains.get(&chain).map_or(0, Vec::len);
            let track_events = events.remove(&id).unwrap_or_default();
            let is_master = id == self.master;
            {
                let track = self.tracks.get_mut(&id).expect("track exists");
                if track.plan.sends.iter().any(|send| send.pre_fader) {
                    track.pre_fader[..frames * 2].copy_from_slice(&track.buffer[..frames * 2]);
                }
                if track.plan.active {
                    for device_index in 0..device_count {
                        let Some(device_id) = self
                            .chains
                            .get(&chain)
                            .and_then(|devices| devices.get(device_index))
                            .copied()
                        else {
                            continue;
                        };
                        let Some(index) = self.processor_index.get(&device_id).copied() else {
                            continue;
                        };
                        if let Some(processor) = self.processors[index].as_mut() {
                            let (input_peak_l, input_peak_r) =
                                stereo_peaks(&track.buffer[..frames * 2]);
                            let status = processor.process(
                                &ctx,
                                &mut track.buffer[..frames * 2],
                                &track_events,
                            );
                            // A device that reports silence has no valid output
                            // for this block. Zeroing avoids passing the
                            // pre-device buffer through as stale audio.
                            if status.produced_silence {
                                track.buffer[..frames * 2].fill(0.0);
                            }
                            // Overload/error output may contain garbage. Drop
                            // non-finite samples here; finite peaks are still
                            // handled by the final safety clip.
                            if status.overloaded {
                                for sample in &mut track.buffer[..frames * 2] {
                                    if !sample.is_finite() {
                                        *sample = 0.0;
                                    }
                                }
                                if self.runtime_faults.len() < MAX_RUNTIME_FAULTS {
                                    self.runtime_faults.push(RuntimeFault { device_id });
                                }
                            }
                            let (output_peak_l, output_peak_r) =
                                stereo_peaks(&track.buffer[..frames * 2]);
                            self.device_meters.insert(
                                device_id,
                                DeviceMeterRuntime {
                                    input_peak_l,
                                    input_peak_r,
                                    output_peak_l,
                                    output_peak_r,
                                },
                            );
                        }
                    }
                }
            }

            self.apply_track_gain_and_pan(id, frames, !is_master);
            if !is_master {
                self.route_track(id, frames);
            }
            if self.measure_meters {
                self.update_meters(id, frames);
            }
        }

        if let Some(master) = self.tracks.get(&self.master) {
            output.copy_from_slice(&master.buffer[..frames * 2]);
        } else {
            output.fill(0.0);
        }
    }
}
