use std::collections::BTreeMap;

use notwig_core::dsp::{NoteEvent, NoteKind};
use notwig_core::{ClipPayload, TrackId};

use super::ActivePlan;

impl ActivePlan {
    pub(super) fn collect_events(
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
            if let Some(prime_sample) = self.prime_at_sample.take() {
                self.prime_notes_covering(prime_sample, &mut events);
            }
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

    /// Add note-ons for notes whose span covers `start_sample` but that started
    /// before it. Notes that are already active are left alone so an ordinary
    /// control publish cannot retrigger them.
    fn prime_notes_covering(
        &mut self,
        start_sample: u64,
        events: &mut BTreeMap<TrackId, Vec<NoteEvent>>,
    ) {
        let map = notwig_core::TempoMap {
            tempo_bpm: self.tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
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
                if start_sample < clip_start || start_sample >= clip_end {
                    continue;
                }
                for note in &midi.notes {
                    if note.start < midi.content_start {
                        continue;
                    }
                    let relative_start = note.start - midi.content_start;
                    let absolute_start_mus = clip.range.start + relative_start;
                    if absolute_start_mus >= clip.range.end {
                        continue;
                    }
                    let absolute_end_mus = (absolute_start_mus + note.length).min(clip.range.end);
                    let note_start = map.mus_to_samples(absolute_start_mus);
                    let note_end = map.mus_to_samples(absolute_end_mus);
                    if note_start >= start_sample || note_end <= start_sample {
                        continue;
                    }
                    let active = self.active_notes.entry(*track_id).or_default();
                    if active.contains_key(&note.id) {
                        continue;
                    }
                    let velocity = note.velocity.0.clamp(0.0, 1.0);
                    active.insert(note.id, (note.pitch, velocity));
                    events.entry(*track_id).or_default().push(NoteEvent {
                        sample_offset: 0,
                        kind: NoteKind::On,
                        note_id: note.id,
                        pitch: note.pitch,
                        velocity,
                    });
                }
            }
        }
    }
}

fn kind_rank(kind: NoteKind) -> u8 {
    match kind {
        NoteKind::Off => 0,
        NoteKind::On => 1,
    }
}
