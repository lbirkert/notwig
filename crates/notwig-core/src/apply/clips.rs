use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{ClipId, NoteId, TrackId};
use crate::protocol::{AudioEdit, DocChange, EngineMsg};
use crate::sequencer::{ClipEntity, ClipKind, ClipPayload, MidiClipData, NoteEntity, sort_lane};
use crate::time::{RangeMus, TimeMus};

use super::audio::{
    audio_slice_window_set, resize_audio_clip_range, trim_audio_end, trim_audio_start,
};
use super::dispatch::{Changes, not_found, schedule_msg};

#[allow(clippy::too_many_arguments)]
pub(super) fn create_clip_command(
    doc: &mut ProjectDoc,
    id: ClipId,
    track_id: TrackId,
    kind: ClipKind,
    range: RangeMus,
    payload: ClipPayload,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    create_clip(doc, id, track_id, kind, range, payload, changes)?;
    Ok(vec![schedule_msg(doc)])
}

#[allow(clippy::too_many_arguments)]
pub(super) fn edit_clip_command(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    range: Option<RangeMus>,
    name: Option<Option<String>>,
    muted: Option<bool>,
    content_start: Option<TimeMus>,
    content_length: Option<TimeMus>,
    audio: Option<&AudioEdit>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    edit_clip(
        doc,
        clip_id,
        range,
        name,
        muted,
        content_start,
        content_length,
        audio,
        changes,
    )?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn move_clip_command(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    track_id: Option<TrackId>,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    move_clip(doc, clip_id, track_id, range, changes)?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn split_clip_command(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    at: TimeMus,
    new_clip_id: ClipId,
    new_note_ids: &[NoteId],
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    split_clip(doc, clip_id, at, new_clip_id, new_note_ids, changes)?;
    Ok(vec![schedule_msg(doc)])
}

#[allow(clippy::too_many_arguments)]
pub(super) fn duplicate_clip_command(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    new_clip_id: ClipId,
    note_id_map: &[(NoteId, NoteId)],
    track_id: Option<TrackId>,
    range: Option<RangeMus>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    duplicate_clip(
        doc,
        clip_id,
        new_clip_id,
        note_id_map,
        track_id,
        range,
        changes,
    )?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn delete_clip_command(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    delete_clip(doc, clip_id, changes)?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn set_clip_color(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    color: Option<crate::time::Color>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    clip.color = color;
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn set_clip_range(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    if range.is_empty() || range.start < TimeMus::zero() {
        return Err(CoreError::ValidationFailed {
            reason: "clip range must be non-empty and start at or after zero".into(),
        });
    }
    let mut clip = existing.clone();
    if let ClipPayload::Midi(midi) = &mut clip.payload {
        resize_midi_clip_start(midi, range.start - existing.range.start);
    }
    clip.range = range;
    resize_audio_clip_range(doc, &existing, &mut clip)?;
    clip.validate(doc)?;
    clear_range(doc, existing.track_id, range, Some(clip_id), changes)?;
    doc.arranger.clips.insert(clip_id, clip);
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn clear_range_command(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if !doc.tracks.contains_key(&track_id) {
        return Err(not_found(&track_id));
    }
    clear_range(doc, track_id, range, None, changes)?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn set_clip_slices(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    slices: &[crate::sequencer::AudioSlice],
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    let ClipPayload::Audio(audio) = &mut clip.payload else {
        return Err(CoreError::ValidationFailed {
            reason: "clip is not an audio clip".into(),
        });
    };
    audio.slices = slices.to_vec();
    audio.slices_enabled = true;
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate().map_err(|err| CoreError::ValidationFailed {
        reason: err.to_string(),
    })?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn set_clip_gain_curve(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    curve: Option<crate::Curve>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    set_clip_curve(doc, clip_id, curve, changes, |audio, curve| {
        audio.gain_curve = curve;
    })
}

pub(super) fn set_clip_pan_curve(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    curve: Option<crate::Curve>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    set_clip_curve(doc, clip_id, curve, changes, |audio, curve| {
        audio.pan_curve = curve;
    })
}

fn set_clip_curve(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    curve: Option<crate::Curve>,
    changes: &mut Changes,
    set: impl FnOnce(&mut crate::sequencer::AudioClipData, Option<crate::Curve>),
) -> Result<Vec<EngineMsg>, CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    let ClipPayload::Audio(audio) = &mut clip.payload else {
        return Err(CoreError::ValidationFailed {
            reason: "clip is not an audio clip".into(),
        });
    };
    set(audio, curve);
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate().map_err(|err| CoreError::ValidationFailed {
        reason: err.to_string(),
    })?;
    Ok(vec![schedule_msg(doc)])
}

#[allow(clippy::too_many_arguments)]
fn create_clip(
    doc: &mut ProjectDoc,
    id: ClipId,
    track_id: TrackId,
    kind: ClipKind,
    range: RangeMus,
    payload: ClipPayload,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&id) {
        return Err(CoreError::DuplicateId { id: id.to_string() });
    }
    let track = doc
        .tracks
        .get(&track_id)
        .ok_or_else(|| not_found(&track_id))?;
    if !track.kind.has_lane() {
        return Err(CoreError::ValidationFailed {
            reason: "only Audio and Instrument tracks host clips".into(),
        });
    }
    let clip = ClipEntity {
        id,
        track_id,
        kind,
        range,
        name: None,
        color: None,
        muted: false,
        payload,
    };
    clip.validate(doc)?;
    clear_range(doc, track_id, range, None, changes)?;
    doc.arranger.clips.insert(id, clip);
    let lane = doc.arranger.lanes.entry(track_id).or_default();
    lane.clips.push(id);
    changes.affected.clips.insert(id);
    changes.push(DocChange::ClipUpserted(id));
    doc.validate()
}

fn clear_range(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    range: RangeMus,
    exclude: Option<ClipId>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let Some(lane) = doc.arranger.lanes.get(&track_id).cloned() else {
        return Ok(());
    };
    let mut remove = Vec::new();
    for other in lane.clips {
        if exclude == Some(other) {
            continue;
        }
        let Some(existing) = doc.arranger.clips.get(&other).cloned() else {
            continue;
        };
        if !existing.range.intersects(range) {
            continue;
        }
        if existing.range.start >= range.start && existing.range.end <= range.end {
            remove.push(other);
        } else if existing.range.start < range.start && existing.range.end > range.end {
            // Erasing across the middle creates a transient right-hand clip;
            // no command carries an ID for it, so mint one internally.
            let right_id = ClipId::from_uuid(crate::ids::new_uuid());
            let new_note_ids = crossing_note_ids(&existing, range.start);
            split_clip(doc, other, range.start, right_id, &new_note_ids, changes)?;
            trim_clip_start(doc, &doc.arranger.clips[&right_id].clone(), range.end)?;
            changes.affected.clips.insert(right_id);
            changes.push(DocChange::ClipUpserted(right_id));
        } else if existing.range.start < range.start {
            trim_clip_end(doc, &existing, range.start)?;
            changes.affected.clips.insert(other);
            changes.push(DocChange::ClipUpserted(other));
        } else {
            trim_clip_start(doc, &existing, range.end)?;
            changes.affected.clips.insert(other);
            changes.push(DocChange::ClipUpserted(other));
        }
    }
    for id in remove {
        remove_clip_internal(doc, id, changes);
    }
    if let Some(lane) = doc.arranger.lanes.get_mut(&track_id) {
        sort_lane(lane, &doc.arranger.clips);
    }
    Ok(())
}

/// Pre-allocates IDs for notes that straddle `at`, in note order.
///
/// `split_clip` consumes these in the same order, so this list and the split
/// loop must stay in lockstep. Internal splits triggered by an erase cannot
/// receive IDs from the caller, so core mints UUIDs here as a deliberate
/// exception to the otherwise caller-supplied-ID rule.
fn crossing_note_ids(clip: &ClipEntity, at: TimeMus) -> Vec<NoteId> {
    let ClipPayload::Midi(midi) = &clip.payload else {
        return Vec::new();
    };
    let split = midi.content_start + (at - clip.range.start);
    midi.notes
        .iter()
        .filter(|note| note.start < split && note.start + note.length > split)
        .map(|_| NoteId::from_uuid(crate::ids::new_uuid()))
        .collect()
}

fn remove_clip_internal(doc: &mut ProjectDoc, id: ClipId, changes: &mut Changes) {
    if let Some(clip) = doc.arranger.clips.remove(&id) {
        if let Some(lane) = doc.arranger.lanes.get_mut(&clip.track_id) {
            lane.clips.retain(|clip_id| *clip_id != id);
        }
        changes.affected.clips.insert(id);
        changes.push(DocChange::ClipRemoved(id));
    }
}

fn resize_midi_clip_start(midi: &mut MidiClipData, delta: TimeMus) {
    if delta == TimeMus::zero() {
        return;
    }
    let desired = midi.content_start + delta;
    if desired >= TimeMus::zero() {
        midi.content_start = desired;
        return;
    }
    let shift = TimeMus::zero() - desired;
    for note in &mut midi.notes {
        note.start = note.start + shift;
    }
    midi.length = midi.length + shift;
    midi.content_start = TimeMus::zero();
}

pub(super) fn trim_clip_start(
    doc: &mut ProjectDoc,
    clip: &ClipEntity,
    new_start: TimeMus,
) -> Result<(), CoreError> {
    let delta = new_start - clip.range.start;
    let mut next = clip.clone();
    next.range = RangeMus::new(new_start, clip.range.end)?;
    match &mut next.payload {
        ClipPayload::Midi(midi) => {
            resize_midi_clip_start(midi, delta);
        }
        ClipPayload::Audio(audio) => {
            trim_audio_start(doc, audio, delta, next.range.length());
        }
    }
    if next.range.is_empty() {
        return Ok(());
    }
    next.validate(doc)?;
    doc.arranger.clips.insert(clip.id, next);
    Ok(())
}

fn trim_clip_end(
    doc: &mut ProjectDoc,
    clip: &ClipEntity,
    new_end: TimeMus,
) -> Result<(), CoreError> {
    let mut next = clip.clone();
    next.range = RangeMus::new(clip.range.start, new_end)?;
    match &mut next.payload {
        ClipPayload::Midi(_) => {}
        ClipPayload::Audio(audio) => {
            let delta = clip.range.end - new_end;
            trim_audio_end(doc, audio, delta, next.range.length());
        }
    }
    if next.range.is_empty() {
        return Ok(());
    }
    next.validate(doc)?;
    doc.arranger.clips.insert(clip.id, next);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn edit_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    range: Option<RangeMus>,
    name: Option<Option<String>>,
    muted: Option<bool>,
    content_start: Option<TimeMus>,
    content_length: Option<TimeMus>,
    audio: Option<&AudioEdit>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let mut next = existing.clone();
    if let Some(value) = name {
        next.name = value;
    }
    if let Some(value) = muted {
        next.muted = value;
    }
    if let Some(new_range) = range {
        if let ClipPayload::Midi(midi) = &mut next.payload {
            resize_midi_clip_start(midi, new_range.start - existing.range.start);
        }
        if let ClipPayload::Audio(data) = &mut next.payload {
            let old = existing.range;
            if data.uses_slices() {
                let map = doc.tempo_map();
                let left_trim = new_range.start - old.start;
                data.slices = audio_slice_window_set(
                    map,
                    data,
                    left_trim,
                    left_trim + new_range.length(),
                    left_trim,
                    false,
                );
                data.slices_enabled = true;
            } else {
                let left_mus = new_range.start - old.start;
                let right_mus = old.end - new_range.end;
                let left_samples = doc.tempo_map().samples_delta(left_mus);
                let right_samples = doc.tempo_map().samples_delta(right_mus);
                let new_start = i128::from(data.source_start_sample) + i128::from(left_samples);
                let new_end = i128::from(data.source_end_sample) - i128::from(right_samples);
                if new_start < 0 || new_end <= new_start {
                    return Err(CoreError::ValidationFailed {
                        reason: "audio trim exceeds source material".into(),
                    });
                }
                data.source_start_sample = u64::try_from(new_start).unwrap_or(0);
                data.source_end_sample = u64::try_from(new_end).unwrap_or(0);
            }
        }
        next.range = new_range;
    }
    match &mut next.payload {
        ClipPayload::Midi(midi) => {
            if let Some(value) = content_start {
                midi.content_start = value;
            }
            if let Some(value) = content_length {
                midi.length = value;
            }
        }
        ClipPayload::Audio(data) => {
            if let Some(edit) = audio {
                if let Some(value) = edit.source_start_sample {
                    data.source_start_sample = value;
                }
                if let Some(value) = edit.source_end_sample {
                    data.source_end_sample = value;
                }
                if let Some(value) = edit.gain_db {
                    data.gain_db = value;
                }
                if let Some(value) = edit.fade_in_samples {
                    data.fade_in_samples = value;
                }
                if let Some(value) = edit.fade_out_samples {
                    data.fade_out_samples = value;
                }
            }
            if range.is_none() && audio.is_some() {
                let duration = data.source_duration();
                next.range = RangeMus::new(
                    existing.range.start,
                    existing.range.start + doc.tempo_map().samples_to_mus(duration),
                )?;
            }
        }
    }
    next.validate(doc)?;
    let range = next.range;
    clear_range(doc, existing.track_id, range, Some(clip_id), changes)?;
    doc.arranger.clips.insert(clip_id, next);
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate()
}

fn move_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    track_id: Option<TrackId>,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let target_track = track_id.unwrap_or(existing.track_id);
    if target_track != existing.track_id
        && let Some(lane) = doc.arranger.lanes.get_mut(&existing.track_id)
    {
        lane.clips.retain(|id| *id != clip_id);
    }
    let mut next = existing.clone();
    next.track_id = target_track;
    next.range = range;
    next.validate(doc)?;
    clear_range(doc, target_track, range, Some(clip_id), changes)?;
    if target_track != existing.track_id {
        doc.arranger
            .lanes
            .entry(target_track)
            .or_default()
            .clips
            .push(clip_id);
    }
    doc.arranger.clips.insert(clip_id, next);
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate()
}

fn split_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    at: TimeMus,
    new_clip_id: ClipId,
    new_note_ids: &[NoteId],
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&new_clip_id) {
        return Err(CoreError::DuplicateId {
            id: new_clip_id.to_string(),
        });
    }
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    if at <= existing.range.start || at >= existing.range.end {
        return Err(CoreError::ValidationFailed {
            reason: "split position must lie strictly inside the clip".into(),
        });
    }
    let split_offset = at - existing.range.start;
    let map = doc.tempo_map();
    let split_samples = u64::try_from(map.samples_delta(split_offset).max(0)).unwrap_or(0);
    let mut right = existing.clone();
    right.id = new_clip_id;
    right.range = RangeMus::new(at, existing.range.end)?;

    match (
        &mut doc.arranger.clips.get_mut(&clip_id).expect("clip").payload,
        &existing.payload,
    ) {
        (ClipPayload::Midi(left_midi), ClipPayload::Midi(orig)) => {
            let split_content = orig.content_start + split_offset;
            let mut left_notes = Vec::new();
            let mut right_notes = Vec::new();
            // Crossing notes are visited in stored note order; `new_note_ids`
            // must therefore line up one-to-one with them in that same order.
            let mut new_ids = new_note_ids.iter().copied();
            for note in &orig.notes {
                let end = note.start + note.length;
                if end <= split_content {
                    left_notes.push(note.clone());
                } else if note.start >= split_content {
                    let mut moved = note.clone();
                    moved.start = note.start - split_content;
                    right_notes.push(moved);
                } else {
                    let new_id = new_ids.next().ok_or_else(|| CoreError::ValidationFailed {
                        reason: "split requires one new note id per crossing note".into(),
                    })?;
                    if orig.notes.iter().any(|n| n.id == new_id) {
                        return Err(CoreError::DuplicateId {
                            id: new_id.to_string(),
                        });
                    }
                    let mut left = note.clone();
                    left.length = split_content - note.start;
                    left_notes.push(left);
                    right_notes.push(NoteEntity {
                        id: new_id,
                        pitch: note.pitch,
                        start: TimeMus::zero(),
                        length: end - split_content,
                        velocity: note.velocity,
                        release_velocity: note.release_velocity,
                        expression: note.expression.clone(),
                    });
                }
            }
            if new_ids.next().is_some() {
                return Err(CoreError::ValidationFailed {
                    reason: "too many new note ids supplied for split".into(),
                });
            }
            left_midi.notes = left_notes;
            left_midi.length = split_content;
            let right_data = MidiClipData {
                length: orig.length - split_content,
                content_start: TimeMus::zero(),
                notes: right_notes,
            };
            right.payload = ClipPayload::Midi(right_data);
        }
        (ClipPayload::Audio(left_audio), ClipPayload::Audio(orig)) => {
            if orig.uses_slices() {
                left_audio.slices = audio_slice_window_set(
                    map,
                    orig,
                    TimeMus::zero(),
                    split_offset,
                    TimeMus::zero(),
                    false,
                );
                left_audio.slices_enabled = true;
                let mut right_audio = orig.clone();
                right_audio.slices = audio_slice_window_set(
                    map,
                    orig,
                    split_offset,
                    existing.range.length(),
                    split_offset,
                    true,
                );
                right_audio.slices_enabled = true;
                right.payload = ClipPayload::Audio(right_audio);
            } else {
                let split_sample = orig.source_start_sample.saturating_add(split_samples);
                left_audio.source_end_sample = split_sample;
                let mut right_audio = orig.clone();
                right_audio.source_start_sample = split_sample;
                right_audio.fade_in_samples = 0;
                left_audio.fade_out_samples = 0;
                right.payload = ClipPayload::Audio(right_audio);
            }
        }
        _ => unreachable!("validated clip kind"),
    }

    right.validate(doc)?;
    doc.arranger
        .clips
        .get_mut(&clip_id)
        .expect("clip")
        .range
        .end = at;
    doc.arranger.clips.insert(new_clip_id, right);
    doc.arranger
        .lanes
        .entry(existing.track_id)
        .or_default()
        .clips
        .push(new_clip_id);
    if let Some(lane) = doc.arranger.lanes.get_mut(&existing.track_id) {
        sort_lane(lane, &doc.arranger.clips);
    }
    changes.affected.clips.insert(clip_id);
    changes.affected.clips.insert(new_clip_id);
    for id in new_note_ids {
        changes.affected.notes.insert(*id);
    }
    changes.push(DocChange::ClipUpserted(clip_id));
    changes.push(DocChange::ClipUpserted(new_clip_id));
    doc.validate()
}

fn duplicate_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    new_clip_id: ClipId,
    note_id_map: &[(NoteId, NoteId)],
    track_id: Option<TrackId>,
    range: Option<RangeMus>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&new_clip_id) {
        return Err(CoreError::DuplicateId {
            id: new_clip_id.to_string(),
        });
    }
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let mut next = existing.clone();
    next.id = new_clip_id;
    next.track_id = track_id.unwrap_or(existing.track_id);
    next.range = range.unwrap_or_else(|| {
        RangeMus::new(
            existing.range.end,
            existing.range.end + existing.range.length(),
        )
        .expect("valid duplicated range")
    });
    if let ClipPayload::Midi(midi) = &mut next.payload {
        let map: std::collections::BTreeMap<NoteId, NoteId> = note_id_map.iter().copied().collect();
        for note in &mut midi.notes {
            let new_id = map
                .get(&note.id)
                .copied()
                .ok_or_else(|| CoreError::ValidationFailed {
                    reason: "duplicate clip requires an id for every note".into(),
                })?;
            note.id = new_id;
        }
    }
    next.validate(doc)?;
    clear_range(doc, next.track_id, next.range, None, changes)?;
    doc.arranger.clips.insert(new_clip_id, next.clone());
    doc.arranger
        .lanes
        .entry(next.track_id)
        .or_default()
        .clips
        .push(new_clip_id);
    changes.affected.clips.insert(new_clip_id);
    changes.push(DocChange::ClipUpserted(new_clip_id));
    doc.validate()
}

fn delete_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if !doc.arranger.clips.contains_key(&clip_id) {
        return Err(not_found(&clip_id));
    }
    remove_clip_internal(doc, clip_id, changes);
    doc.validate()
}
