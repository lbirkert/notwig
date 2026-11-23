use std::collections::BTreeSet;

use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{ClipId, NoteId};
use crate::protocol::{DocChange, EngineMsg};
use crate::sequencer::{ClipPayload, MidiClipData, NoteEntity};

use super::dispatch::{Changes, not_found, schedule_msg};

pub(super) fn set_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note: NoteEntity,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let note_id = note.id;
    let midi = midi_clip_mut(doc, clip_id)?;
    if let Some(existing) = midi.notes.iter_mut().find(|n| n.id == note.id) {
        *existing = note;
    } else {
        midi.notes.push(note);
    }
    sort_notes(&mut midi.notes);
    midi.validate()?;
    record_note_change(changes, clip_id, &[note_id]);
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn create_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note: NoteEntity,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let note_id = note.id;
    let midi = midi_clip_mut(doc, clip_id)?;
    if midi.notes.iter().any(|existing| existing.id == note_id) {
        return Err(CoreError::DuplicateId {
            id: note_id.to_string(),
        });
    }
    note.validate()?;
    midi.notes.push(note);
    sort_notes(&mut midi.notes);
    midi.validate()?;
    record_note_change(changes, clip_id, &[note_id]);
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn set_notes(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    notes: &[NoteEntity],
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let mut ids = BTreeSet::new();
    for note in notes {
        if !ids.insert(note.id) {
            return Err(CoreError::DuplicateId {
                id: note.id.to_string(),
            });
        }
    }
    let midi = midi_clip_mut(doc, clip_id)?;
    for note in notes {
        if let Some(existing) = midi.notes.iter_mut().find(|n| n.id == note.id) {
            *existing = note.clone();
        } else {
            midi.notes.push(note.clone());
        }
    }
    sort_notes(&mut midi.notes);
    midi.validate()?;
    let ids: Vec<NoteId> = ids.into_iter().collect();
    record_note_change(changes, clip_id, &ids);
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn delete_notes(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note_ids: &[NoteId],
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if note_ids.is_empty() {
        return Ok(vec![schedule_msg(doc)]);
    }
    let requested: BTreeSet<NoteId> = note_ids.iter().copied().collect();
    let midi = midi_clip_mut(doc, clip_id)?;
    for id in &requested {
        if !midi.notes.iter().any(|note| note.id == *id) {
            return Err(not_found(id));
        }
    }
    midi.notes.retain(|note| !requested.contains(&note.id));
    midi.validate()?;
    let ids: Vec<NoteId> = requested.into_iter().collect();
    record_note_change(changes, clip_id, &ids);
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn delete_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note_id: NoteId,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    delete_notes(doc, clip_id, &[note_id], changes)
}

fn midi_clip_mut(doc: &mut ProjectDoc, clip_id: ClipId) -> Result<&mut MidiClipData, CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    match &mut clip.payload {
        ClipPayload::Midi(midi) => Ok(midi),
        ClipPayload::Audio(_) => Err(CoreError::ValidationFailed {
            reason: "notes can only be edited in MIDI clips".into(),
        }),
    }
}

fn sort_notes(notes: &mut [NoteEntity]) {
    notes.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(a.pitch.cmp(&b.pitch))
            .then(a.id.cmp(&b.id))
    });
}

fn record_note_change(changes: &mut Changes, clip_id: ClipId, note_ids: &[NoteId]) {
    changes.affected.clips.insert(clip_id);
    for id in note_ids {
        changes.affected.notes.insert(*id);
    }
    changes.push(DocChange::ClipUpserted(clip_id));
}
