use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use notwig_core::{
    AssetId, AudioAssetBuffer, ClipPayload, Db, DeviceChainId, DeviceId, NoteId, Pan, ParamId,
    ParamValue, PlaybackSchedule, SendId, TrackId, TrackOutput,
};
use notwig_dsp::{
    CompiledPlan, DeviceProcessor, NoteEvent, NoteKind, PlanTrack, ProcessContext, RenderConfig,
    TransportInfo,
};

use crate::control::PreviewNote;
use crate::engine::{EngineConfig, EngineError};
use notwig_dsp::DeviceFactory;

const PREVIEW_POOL_CAPACITY: usize = 32;

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

#[derive(Debug)]
pub struct TrackRuntime {
    pub plan: PlanTrack,
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
        Self {
            input_delay: DelayLine::new(plan.input_latency),
            main_delay: DelayLine::new(main_delay_samples),
            send_delays,
            plan,
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
}

pub struct ActivePlan {
    config: RenderConfig,
    master: TrackId,
    order: Vec<TrackId>,
    tracks: BTreeMap<TrackId, TrackRuntime>,
    chains: BTreeMap<DeviceChainId, Vec<DeviceId>>,
    processors: Vec<Option<Box<dyn DeviceProcessor>>>,
    processor_index: BTreeMap<DeviceId, usize>,
    device_meters: BTreeMap<DeviceId, DeviceMeterRuntime>,
    schedule: Arc<PlaybackSchedule>,
    assets: Arc<BTreeMap<AssetId, Arc<AudioAssetBuffer>>>,
    active_notes: BTreeMap<TrackId, BTreeMap<NoteId, (u8, f32)>>,
    preview_pool: [Option<PreviewSlot>; PREVIEW_POOL_CAPACITY],
    panic_pending: bool,
    tempo_bpm: f64,
    sample_rate: u32,
    route_scratch: Vec<f32>,
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
        let order = processing_order(plan)?;
        let mut tracks = BTreeMap::new();
        for (id, track) in &plan.tracks {
            let main_delay = output_target(track, plan.master)
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
            if let Some(mut processor) = build_processor(plan, device_id, factory) {
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
        Ok(Self {
            config,
            master: plan.master,
            order,
            tracks,
            chains,
            processors,
            processor_index,
            device_meters: BTreeMap::new(),
            schedule: Arc::clone(schedule),
            assets,
            active_notes: BTreeMap::new(),
            preview_pool: [None; PREVIEW_POOL_CAPACITY],
            panic_pending: false,
            tempo_bpm: plan.config.tempo_bpm,
            sample_rate: plan.config.sample_rate,
            route_scratch: vec![0.0; config.max_block_size.max(1) * 2],
            measure_meters: true,
        })
    }

    pub fn set_measure_meters(&mut self, measure: bool) {
        self.measure_meters = measure;
    }

    pub fn reset(&mut self) {
        self.active_notes.clear();
        self.preview_pool = [None; PREVIEW_POOL_CAPACITY];
        self.panic_pending = false;
        for track in self.tracks.values_mut() {
            track.reset();
        }
        for processor in self.processors.iter_mut().flatten() {
            processor.reset();
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

    pub fn emit_all_notes_off(&mut self) {
        self.active_notes.clear();
        self.panic_pending = false;
    }

    pub fn set_param(&mut self, device_id: DeviceId, param_id: ParamId, value: &ParamValue) {
        if let Some(index) = self.processor_index.get(&device_id).copied() {
            if let Some(processor) = self.processors[index].as_mut() {
                processor.set_param(param_id, value);
            }
            return;
        }
        for processor in self.processors.iter_mut().flatten() {
            if processor.set_nested_param(device_id, param_id, value) {
                break;
            }
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

    pub fn meter_snapshot(&self) -> Vec<(TrackId, f32, f32, f32, f32)> {
        self.tracks
            .iter()
            .map(|(id, track)| (*id, track.peak_l, track.peak_r, track.rms_l, track.rms_r))
            .collect()
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
                );
                written += count;
            }
            return Ok(());
        }
        self.render_inner(start_sample, frames, output, playing, include_clips);
        Ok(())
    }

    fn render_inner(
        &mut self,
        start_sample: u64,
        frames: usize,
        output: &mut [f32],
        playing: bool,
        include_clips: bool,
    ) {
        for track in self.tracks.values_mut() {
            track.buffer[..frames * 2].fill(0.0);
            track.pre_fader[..frames * 2].fill(0.0);
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
                            let _ = processor.process(
                                &ctx,
                                &mut track.buffer[..frames * 2],
                                &track_events,
                            );
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

    fn render_audio_clips(&mut self, start_sample: u64, frames: usize) {
        let map = notwig_core::TempoMap {
            tempo_bpm: self.tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
        for (track_id, clips) in &self.schedule.tracks {
            let Some(track) = self.tracks.get_mut(track_id) else {
                continue;
            };
            if !track.plan.active {
                continue;
            }
            for clip in clips {
                let ClipPayload::Audio(audio) = &clip.payload else {
                    continue;
                };
                let clip_start = map.mus_to_samples(clip.range.start);
                let clip_end = map.mus_to_samples(clip.range.end);
                if clip_end <= start_sample || clip_start >= start_sample + frames as u64 {
                    continue;
                }
                if audio.uses_slices() {
                    let clip_start = i64::try_from(clip_start).unwrap_or(i64::MAX);
                    let clip_end = i64::try_from(clip_end).unwrap_or(i64::MAX);
                    for slice in &audio.slices {
                        let Some(asset) =
                            self.assets.get(&slice.asset_id.unwrap_or(audio.asset_id))
                        else {
                            continue;
                        };
                        if asset.channels == 0 {
                            continue;
                        }
                        let source_len =
                            i64::try_from(slice.duration_samples()).unwrap_or(i64::MAX);
                        if source_len <= 0 {
                            continue;
                        }
                        let slice_start = clip_start + map.samples_delta(slice.start);
                        let slice_end = slice_start.saturating_add(source_len);
                        let visible_start = slice_start.max(clip_start).max(0);
                        let visible_end = slice_end.min(clip_end);
                        if visible_end <= visible_start {
                            continue;
                        }
                        let offset = u64::try_from(visible_start - slice_start).unwrap_or(0);
                        let audio_offset = offset.max(slice.silent_prefix_samples);
                        let region_start = slice_start
                            .saturating_add(i64::try_from(audio_offset).unwrap_or(i64::MAX));
                        if region_start >= visible_end
                            || slice.source_start_sample >= slice.source_end_sample
                        {
                            continue;
                        }
                        let source_start = slice
                            .source_start_sample
                            .saturating_add(audio_offset - slice.silent_prefix_samples);
                        if source_start >= slice.source_end_sample {
                            continue;
                        }
                        render_audio_region(
                            track,
                            asset,
                            u64::try_from(region_start).unwrap_or(0),
                            u64::try_from(visible_end).unwrap_or(0),
                            source_start,
                            slice.source_end_sample,
                            slice.gain_db.to_gain().0,
                            0,
                            0,
                            start_sample,
                            frames,
                        );
                    }
                } else {
                    let Some(asset) = self.assets.get(&audio.asset_id) else {
                        continue;
                    };
                    if asset.channels == 0 {
                        continue;
                    }
                    render_audio_region(
                        track,
                        asset,
                        clip_start,
                        clip_end,
                        audio.source_start_sample,
                        audio.source_end_sample,
                        audio.gain_db.to_gain().0,
                        audio.fade_in_samples,
                        audio.fade_out_samples,
                        start_sample,
                        frames,
                    );
                }
            }
        }
    }

    fn collect_events(
        &mut self,
        start_sample: u64,
        frames: usize,
        include_clips: bool,
    ) -> BTreeMap<TrackId, Vec<NoteEvent>> {
        let map = notwig_core::TempoMap {
            tempo_bpm: self.tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
        let mut events: BTreeMap<TrackId, Vec<NoteEvent>> = BTreeMap::new();
        if self.panic_pending {
            for (track, notes) in &self.active_notes {
                let list = events.entry(*track).or_default();
                for (note_id, (pitch, velocity)) in notes {
                    list.push(NoteEvent {
                        sample_offset: 0,
                        kind: NoteKind::Off,
                        note_id: *note_id,
                        pitch: *pitch,
                        velocity: *velocity,
                    });
                }
            }
            for index in 0..self.preview_pool.len() {
                let Some(preview) = self.preview_pool[index].take() else {
                    continue;
                };
                if !preview.on_emitted || !self.tracks.contains_key(&preview.track_id) {
                    continue;
                }
                events.entry(preview.track_id).or_default().push(NoteEvent {
                    sample_offset: 0,
                    kind: NoteKind::Off,
                    note_id: preview.note_id,
                    pitch: preview.pitch,
                    velocity: preview.velocity,
                });
            }
            self.active_notes.clear();
            self.panic_pending = false;
        }
        let block_end = start_sample + frames as u64;
        if include_clips {
            for (track_id, clips) in &self.schedule.tracks {
                if !self
                    .tracks
                    .get(track_id)
                    .is_some_and(|track| track.plan.active)
                {
                    continue;
                }
                for clip in clips {
                    let ClipPayload::Midi(midi) = &clip.payload else {
                        continue;
                    };
                    let clip_start = map.mus_to_samples(clip.range.start);
                    let clip_end = map.mus_to_samples(clip.range.end);
                    if clip_end <= start_sample || clip_start >= block_end {
                        continue;
                    }
                    for note in &midi.notes {
                        if note.start < midi.content_start {
                            continue;
                        }
                        let relative_start = note.start - midi.content_start;
                        let absolute_start_mus = clip.range.start + relative_start;
                        let absolute_end_mus =
                            (absolute_start_mus + note.length).min(clip.range.end);
                        if absolute_start_mus >= clip.range.end {
                            continue;
                        }
                        let note_start = map.mus_to_samples(absolute_start_mus);
                        let note_end = map.mus_to_samples(absolute_end_mus);
                        let velocity = note.velocity.0.clamp(0.0, 1.0);
                        let list = events.entry(*track_id).or_default();
                        if note_start >= start_sample && note_start < block_end {
                            list.push(NoteEvent {
                                sample_offset: (note_start - start_sample) as u32,
                                kind: NoteKind::On,
                                note_id: note.id,
                                pitch: note.pitch,
                                velocity,
                            });
                            self.active_notes
                                .entry(*track_id)
                                .or_default()
                                .insert(note.id, (note.pitch, velocity));
                        }
                        if note_end > start_sample && note_end <= block_end && note_end > note_start
                        {
                            list.push(NoteEvent {
                                sample_offset: (note_end.saturating_sub(start_sample))
                                    .min(frames.saturating_sub(1) as u64)
                                    as u32,
                                kind: NoteKind::Off,
                                note_id: note.id,
                                pitch: note.pitch,
                                velocity,
                            });
                            if let Some(active) = self.active_notes.get_mut(track_id) {
                                active.remove(&note.id);
                            }
                        }
                    }
                }
            }
        }
        let frames_u32 = u32::try_from(frames).unwrap_or(u32::MAX);
        for index in 0..self.preview_pool.len() {
            let Some(mut preview) = self.preview_pool[index] else {
                continue;
            };
            if !self.tracks.contains_key(&preview.track_id) {
                self.preview_pool[index] = None;
                continue;
            }
            let list = events.entry(preview.track_id).or_default();
            let just_started = !preview.on_emitted;
            if just_started {
                list.push(NoteEvent {
                    sample_offset: 0,
                    kind: NoteKind::On,
                    note_id: preview.note_id,
                    pitch: preview.pitch,
                    velocity: preview.velocity,
                });
                preview.on_emitted = true;
            }
            if preview.frames_until_off <= frames_u32 {
                let offset = preview
                    .frames_until_off
                    .min(frames.saturating_sub(1) as u32);
                let offset = if just_started && offset == 0 && frames > 1 {
                    1
                } else {
                    offset
                };
                list.push(NoteEvent {
                    sample_offset: offset,
                    kind: NoteKind::Off,
                    note_id: preview.note_id,
                    pitch: preview.pitch,
                    velocity: preview.velocity,
                });
                self.preview_pool[index] = None;
            } else {
                preview.frames_until_off -= frames_u32;
                self.preview_pool[index] = Some(preview);
            }
        }
        for list in events.values_mut() {
            list.sort_by(|a, b| {
                a.sample_offset
                    .cmp(&b.sample_offset)
                    .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
                    .then_with(|| a.note_id.cmp(&b.note_id))
            });
        }
        events
    }

    fn apply_track_gain_and_pan(&mut self, id: TrackId, frames: usize, apply_pan: bool) {
        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };
        let audible = track.plan.solo_effective && !track.plan.mute;
        let gain = if audible {
            track.plan.volume_db.to_gain().0
        } else {
            0.0
        };
        let (pan_l, pan_r) = if apply_pan {
            track.plan.pan.gains()
        } else {
            (1.0, 1.0)
        };
        for frame in track.buffer[..frames * 2].chunks_exact_mut(2) {
            frame[0] *= gain * pan_l;
            frame[1] *= gain * pan_r;
        }
    }

    fn route_track(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get(&id) else {
            return;
        };
        let target = output_target(&track.plan, self.master);
        let send_count = track.plan.sends.len();
        if let Some(target_id) = target {
            if target_id != id {
                self.mix_main(id, target_id, frames);
            }
        }
        for index in 0..send_count {
            let Some(send) = self
                .tracks
                .get(&id)
                .and_then(|track| track.plan.sends.get(index))
                .cloned()
            else {
                continue;
            };
            if !send.enabled || send.target == id {
                continue;
            }
            self.mix_send(id, &send, frames);
        }
    }

    fn mix_main(&mut self, source: TrackId, target: TrackId, frames: usize) {
        if source == target || !self.tracks.contains_key(&target) {
            return;
        }
        // Pad only this connection by the difference between the destination's
        // input label and the source's output label. Downstream connections pad
        // themselves, so nested groups are not compensated twice.
        let delay_frames = self
            .tracks
            .get(&source)
            .zip(self.tracks.get(&target))
            .map_or(0, |(source, target)| {
                target
                    .plan
                    .input_latency
                    .saturating_sub(source.plan.output_latency)
            });
        {
            let Some(source_track) = self.tracks.get_mut(&source) else {
                return;
            };
            let TrackRuntime {
                buffer, main_delay, ..
            } = source_track;
            let scratch = &mut self.route_scratch[..frames * 2];
            if delay_frames == 0 {
                scratch.copy_from_slice(&buffer[..frames * 2]);
            } else {
                main_delay.process(&buffer[..frames * 2], scratch, frames);
            }
        }
        if let Some(target_track) = self.tracks.get_mut(&target) {
            for (dst, sample) in target_track.buffer[..frames * 2]
                .iter_mut()
                .zip(self.route_scratch[..frames * 2].iter())
            {
                *dst += *sample;
            }
        }
    }

    fn mix_send(&mut self, source: TrackId, send: &notwig_dsp::PlanSend, frames: usize) {
        let Some(source_track) = self.tracks.get_mut(&source) else {
            return;
        };
        // Pre-fader sends carry the track input label; post-fader sends carry the
        // track output label. The plan stores the padding to the destination's
        // input label so the send and direct paths stay phase-coherent.
        let TrackRuntime {
            buffer,
            pre_fader,
            send_delays,
            ..
        } = source_track;
        let input = if send.pre_fader {
            &pre_fader[..frames * 2]
        } else {
            &buffer[..frames * 2]
        };
        let scratch = &mut self.route_scratch[..frames * 2];
        if send.latency_samples == 0 {
            scratch.copy_from_slice(input);
        } else if let Some(delay) = send_delays.get_mut(&send.id) {
            delay.process(input, scratch, frames);
        } else {
            scratch.copy_from_slice(input);
        }
        let gain = send.amount_db.to_gain().0;
        if let Some(target) = self.tracks.get_mut(&send.target) {
            for (dst, sample) in target.buffer[..frames * 2]
                .iter_mut()
                .zip(self.route_scratch[..frames * 2].iter())
            {
                *dst += *sample * gain;
            }
        }
    }

    fn update_meters(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;
        for frame in track.buffer[..frames * 2].chunks_exact(2) {
            peak_l = peak_l.max(frame[0].abs());
            peak_r = peak_r.max(frame[1].abs());
            sum_l += f64::from(frame[0]) * f64::from(frame[0]);
            sum_r += f64::from(frame[1]) * f64::from(frame[1]);
        }
        let frames = frames.max(1) as f64;
        track.peak_l = peak_l;
        track.peak_r = peak_r;
        track.rms_l = (sum_l / frames).sqrt() as f32;
        track.rms_r = (sum_r / frames).sqrt() as f32;
    }
}

fn stereo_peaks(samples: &[f32]) -> (f32, f32) {
    let mut peak_l = 0.0_f32;
    let mut peak_r = 0.0_f32;
    for frame in samples.chunks_exact(2) {
        peak_l = peak_l.max(frame[0].abs());
        peak_r = peak_r.max(frame[1].abs());
    }
    (peak_l, peak_r)
}

fn kind_rank(kind: NoteKind) -> u8 {
    match kind {
        NoteKind::Off => 0,
        NoteKind::On => 1,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_audio_region(
    track: &mut TrackRuntime,
    asset: &AudioAssetBuffer,
    region_start: u64,
    region_end: u64,
    source_start: u64,
    source_end: u64,
    gain: f32,
    fade_in_samples: u64,
    fade_out_samples: u64,
    start_sample: u64,
    frames: usize,
) {
    for i in 0..frames {
        let absolute = start_sample + i as u64;
        if absolute < region_start || absolute >= region_end {
            continue;
        }
        let source = source_start + (absolute - region_start);
        if source >= source_end {
            continue;
        }
        let fade_in = if fade_in_samples == 0 {
            1.0
        } else {
            ((source - source_start) as f32 / fade_in_samples as f32).min(1.0)
        };
        let fade_out = if fade_out_samples == 0 {
            1.0
        } else {
            ((source_end - source) as f32 / fade_out_samples as f32).min(1.0)
        };
        let envelope = fade_in.min(fade_out) * gain;
        let (left, right) = sample_frame(asset, source);
        let idx = i * 2;
        track.buffer[idx] += left * envelope;
        track.buffer[idx + 1] += right * envelope;
    }
}

fn sample_frame(asset: &AudioAssetBuffer, frame: u64) -> (f32, f32) {
    let channels = usize::from(asset.channels.max(1));
    let start = frame as usize * channels;
    match channels {
        1 => {
            let value = asset.samples.get(start).copied().unwrap_or(0.0);
            (value, value)
        }
        _ => (
            asset.samples.get(start).copied().unwrap_or(0.0),
            asset.samples.get(start + 1).copied().unwrap_or(0.0),
        ),
    }
}

fn output_target(track: &PlanTrack, master: TrackId) -> Option<TrackId> {
    match track.output {
        TrackOutput::Track(target) => Some(target),
        TrackOutput::Default => track.parent.or((track.id != master).then_some(master)),
        TrackOutput::None => None,
    }
}

fn processing_order(plan: &CompiledPlan) -> Result<Vec<TrackId>, EngineError> {
    let mut adjacency: BTreeMap<TrackId, BTreeSet<TrackId>> = BTreeMap::new();
    let mut indegree: BTreeMap<TrackId, usize> = plan.tracks.keys().map(|id| (*id, 0)).collect();
    let mut add_edge = |from: TrackId, to: TrackId| {
        if from == to || !plan.tracks.contains_key(&to) || !plan.tracks.contains_key(&from) {
            return;
        }
        let set = adjacency.entry(from).or_default();
        if set.insert(to) {
            *indegree.entry(to).or_insert(0) += 1;
        }
    };
    for track in plan.tracks.values() {
        let target = match track.output {
            TrackOutput::Track(target) => Some(target),
            TrackOutput::Default => track
                .parent
                .or((track.id != plan.master).then_some(plan.master)),
            TrackOutput::None => None,
        };
        if let Some(target) = target {
            add_edge(track.id, target);
        }
        for send in &track.sends {
            if send.enabled {
                add_edge(track.id, send.target);
            }
        }
    }
    let rank: BTreeMap<TrackId, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut ready: BTreeSet<(usize, TrackId)> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| (rank.get(id).copied().unwrap_or(usize::MAX), *id))
        .collect();
    let mut order = Vec::with_capacity(plan.tracks.len());
    while let Some((_, id)) = ready.iter().next().copied() {
        ready.remove(&(rank.get(&id).copied().unwrap_or(usize::MAX), id));
        order.push(id);
        if let Some(targets) = adjacency.get(&id) {
            for target in targets.clone() {
                if let Some(degree) = indegree.get_mut(&target) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        ready.insert((rank.get(&target).copied().unwrap_or(usize::MAX), target));
                    }
                }
            }
        }
    }
    if order.len() != plan.tracks.len() {
        return Err(EngineError::Cycle);
    }
    Ok(order)
}

fn build_processor(
    plan: &CompiledPlan,
    device_id: DeviceId,
    factory: &dyn DeviceFactory,
) -> Option<Box<dyn DeviceProcessor>> {
    let device = plan.devices.get(&device_id)?;
    if device.bypass {
        return None;
    }
    if device.type_id == notwig_dsp::stock::CHAIN_TYPE_ID {
        let mut chain = notwig_dsp::stock::ChainProcessor::new(&device.spec);
        for param in &device.spec.params {
            chain.set_param(param.id, &param.default);
        }
        for (id, value) in &device.param_values {
            chain.set_param(*id, value);
        }
        for child_chain in &device.child_chains {
            if let Some(children) = plan.chains.get(child_chain) {
                for child_id in children {
                    if let Some(child) = build_processor(plan, *child_id, factory) {
                        chain.add_child(*child_id, child);
                    }
                }
            }
        }
        return Some(Box::new(chain));
    }
    let mut processor = factory.create(plan, device)?;
    for (id, value) in &device.param_values {
        processor.set_param(*id, value);
    }
    Some(processor)
}
