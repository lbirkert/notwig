use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use notwig_core::{
    AssetId, AudioAssetBuffer, ClipPayload, Db, DeviceChainId, DeviceId, NoteId, Pan, ParamId,
    ParamValue, PlaybackSchedule, TrackId, TrackOutput,
};
use notwig_dsp::{
    CompiledPlan, DeviceProcessor, NoteEvent, NoteKind, PlanTrack, ProcessContext, RenderConfig,
    TransportInfo,
};

use crate::engine::{EngineConfig, EngineError};
use crate::factory::DeviceFactory;

#[derive(Debug)]
pub struct TrackRuntime {
    pub plan: PlanTrack,
    pub buffer: Vec<f32>,
    pub pre_fader: Vec<f32>,
    pub delay: Vec<f32>,
    pub delay_pos: usize,
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

impl TrackRuntime {
    fn new(plan: PlanTrack, config: RenderConfig, total_latency: u32) -> Self {
        let frames = config.max_block_size.max(1);
        Self {
            plan,
            buffer: vec![0.0; frames * 2],
            pre_fader: vec![0.0; frames * 2],
            delay: if total_latency > 0 {
                vec![0.0; total_latency as usize * 2]
            } else {
                Vec::new()
            },
            delay_pos: 0,
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pre_fader.fill(0.0);
        self.delay.fill(0.0);
        self.delay_pos = 0;
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
    schedule: Arc<PlaybackSchedule>,
    assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    active_notes: BTreeMap<TrackId, BTreeMap<NoteId, (u8, f32)>>,
    panic_pending: bool,
    tempo_bpm: f64,
    sample_rate: u32,
}

impl ActivePlan {
    pub(crate) fn build(
        plan: &Arc<CompiledPlan>,
        schedule: &Arc<PlaybackSchedule>,
        assets: &BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
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
            tracks.insert(
                *id,
                TrackRuntime::new(track.clone(), config, plan.total_latency),
            );
        }
        let mut processors = Vec::new();
        let mut processor_index = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for track in plan.tracks.values() {
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
            schedule: Arc::clone(schedule),
            assets: assets.clone(),
            active_notes: BTreeMap::new(),
            panic_pending: false,
            tempo_bpm: plan.config.tempo_bpm,
            sample_rate: plan.config.sample_rate,
        })
    }

    pub fn reset(&mut self) {
        self.active_notes.clear();
        self.panic_pending = false;
        for track in self.tracks.values_mut() {
            track.reset();
        }
        for processor in self.processors.iter_mut().flatten() {
            processor.reset();
        }
    }

    pub(crate) fn set_schedule(&mut self, schedule: Arc<PlaybackSchedule>) {
        self.schedule = schedule;
    }

    pub(crate) fn set_asset(&mut self, asset_id: AssetId, buffer: Arc<AudioAssetBuffer>) {
        self.assets.insert(asset_id, buffer);
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

    pub fn render(
        &mut self,
        start_sample: u64,
        frames: usize,
        output: &mut [f32],
        playing: bool,
    ) -> Result<(), EngineError> {
        for track in self.tracks.values_mut() {
            track.buffer[..frames * 2].fill(0.0);
            track.pre_fader[..frames * 2].fill(0.0);
        }

        self.render_audio_clips(start_sample, frames);
        let mut events = self.collect_events(start_sample, frames);

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

        let order = self.order.clone();
        for id in &order {
            let Some(chain) = self.tracks.get(id).map(|track| track.plan.main_chain) else {
                continue;
            };
            let device_ids = self.chains.get(&chain).cloned().unwrap_or_default();
            let track_events = events.remove(id).unwrap_or_default();
            let is_master = *id == self.master;
            {
                let track = self.tracks.get_mut(id).expect("track exists");
                if track.plan.sends.iter().any(|send| send.pre_fader) {
                    track.pre_fader[..frames * 2].copy_from_slice(&track.buffer[..frames * 2]);
                }
                for device_id in device_ids {
                    let Some(index) = self.processor_index.get(&device_id).copied() else {
                        continue;
                    };
                    if let Some(processor) = self.processors[index].as_mut() {
                        let _ =
                            processor.process(&ctx, &mut track.buffer[..frames * 2], &track_events);
                    }
                }
            }

            self.apply_track_gain_and_pan(*id, frames);
            if !is_master {
                self.route_track(*id, frames);
            }
            self.update_meters(*id, frames);
        }

        if let Some(master) = self.tracks.get(&self.master) {
            output.copy_from_slice(&master.buffer[..frames * 2]);
        } else {
            output.fill(0.0);
        }
        Ok(())
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
            for clip in clips {
                let ClipPayload::Audio(audio) = &clip.payload else {
                    continue;
                };
                let Some(asset) = self.assets.get(&audio.asset_id) else {
                    continue;
                };
                if asset.channels == 0 {
                    continue;
                }
                let clip_start = map.mus_to_samples(clip.range.start);
                let clip_end = map.mus_to_samples(clip.range.end);
                if clip_end <= start_sample || clip_start >= start_sample + frames as u64 {
                    continue;
                }
                let gain = audio.gain_db.to_gain().0;
                let source_start = audio.source_start_sample;
                let source_end = audio.source_end_sample;
                for i in 0..frames {
                    let absolute = start_sample + i as u64;
                    if absolute < clip_start || absolute >= clip_end {
                        continue;
                    }
                    let source = source_start + (absolute - clip_start);
                    if source >= source_end {
                        continue;
                    }
                    let fade_in = if audio.fade_in_samples == 0 {
                        1.0
                    } else {
                        ((source - source_start) as f32 / audio.fade_in_samples as f32).min(1.0)
                    };
                    let fade_out = if audio.fade_out_samples == 0 {
                        1.0
                    } else {
                        ((source_end - source) as f32 / audio.fade_out_samples as f32).min(1.0)
                    };
                    let envelope = fade_in.min(fade_out) * gain;
                    let (left, right) = sample_frame(asset, source);
                    let idx = i * 2;
                    track.buffer[idx] += left * envelope;
                    track.buffer[idx + 1] += right * envelope;
                }
            }
        }
    }

    fn collect_events(
        &mut self,
        start_sample: u64,
        frames: usize,
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
            self.active_notes.clear();
            self.panic_pending = false;
        }
        let block_end = start_sample + frames as u64;
        for (track_id, clips) in &self.schedule.tracks {
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
                    let absolute_end_mus = (absolute_start_mus + note.length).min(clip.range.end);
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
                    if note_end > start_sample && note_end <= block_end && note_end > note_start {
                        list.push(NoteEvent {
                            sample_offset: (note_end.saturating_sub(start_sample)) as u32,
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

    fn apply_track_gain_and_pan(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };
        let audible = track.plan.solo_effective && !track.plan.mute;
        let gain = if audible {
            track.plan.volume_db.to_gain().0
        } else {
            0.0
        };
        let (pan_l, pan_r) = track.plan.pan.gains();
        for frame in track.buffer[..frames * 2].chunks_exact_mut(2) {
            frame[0] *= gain * pan_l;
            frame[1] *= gain * pan_r;
        }
    }

    fn route_track(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get(&id) else {
            return;
        };
        let target = match track.plan.output {
            TrackOutput::Track(target) => Some(target),
            TrackOutput::Default => track
                .plan
                .parent
                .or((id != self.master).then_some(self.master)),
            TrackOutput::None => None,
        };
        let sends = track.plan.sends.clone();
        if let Some(target_id) = target {
            if target_id != id {
                self.mix_main(id, target_id, frames);
            }
        }
        for send in sends {
            if !send.enabled || send.target == id {
                continue;
            }
            let gain = send.amount_db.to_gain().0;
            let source_is_pre = send.pre_fader;
            let Some(source_track) = self.tracks.get(&id) else {
                continue;
            };
            let source: Vec<f32> = if source_is_pre {
                source_track.pre_fader[..frames * 2].to_vec()
            } else {
                source_track.buffer[..frames * 2].to_vec()
            };
            if let Some(target) = self.tracks.get_mut(&send.target) {
                for (dst, src) in target.buffer[..frames * 2].iter_mut().zip(source.iter()) {
                    *dst += *src * gain;
                }
            }
        }
    }

    fn mix_main(&mut self, source: TrackId, target: TrackId, frames: usize) {
        if source == target || !self.tracks.contains_key(&target) {
            return;
        }
        let delay_frames = self.tracks.get(&source).map_or(0, |track| {
            self.max_latency()
                .saturating_sub(track.plan.latency_to_master)
        });
        let src = self
            .tracks
            .get(&source)
            .map(|track| track.buffer[..frames * 2].to_vec())
            .unwrap_or_default();
        let routed = if delay_frames == 0 {
            src
        } else {
            let Some(mut source_track) = self.tracks.remove(&source) else {
                return;
            };
            let delay_len = source_track.delay.len() / 2;
            if delay_len == 0 {
                self.tracks.insert(source, source_track);
                return;
            }
            let mut delayed = vec![0.0; frames * 2];
            let mut pos = source_track.delay_pos % delay_len;
            for frame in 0..frames {
                for channel in 0..2 {
                    let index = pos * 2 + channel;
                    delayed[frame * 2 + channel] = source_track.delay[index];
                    source_track.delay[index] = src[frame * 2 + channel];
                }
                pos = (pos + 1) % delay_len;
            }
            source_track.delay_pos = pos;
            self.tracks.insert(source, source_track);
            delayed
        };
        if let Some(target_track) = self.tracks.get_mut(&target) {
            for (dst, sample) in target_track.buffer[..frames * 2]
                .iter_mut()
                .zip(routed.iter())
            {
                *dst += *sample;
            }
        }
    }

    fn max_latency(&self) -> u32 {
        self.tracks
            .values()
            .map(|track| track.plan.latency_to_master)
            .max()
            .unwrap_or(0)
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

fn kind_rank(kind: NoteKind) -> u8 {
    match kind {
        NoteKind::Off => 0,
        NoteKind::On => 1,
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
