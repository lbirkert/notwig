use std::collections::BTreeSet;

use notwig_core::{ClipEntity, MidiClipData, NoteId, TrackId};

use super::super::geometry::beat_from_content_time;
use super::super::state::{MidiEditorState, capped_preview_length, preview_highlight_seconds};
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;

pub(super) fn played_pitches(
    midi: &MidiClipData,
    clip: &ClipEntity,
    runtime: &RuntimeView,
) -> BTreeSet<u8> {
    let mut pitches = BTreeSet::new();
    if !runtime.playing {
        return pitches;
    }
    for note in &midi.notes {
        let start = beat_from_content_time(note.start, clip.range.start, midi.content_start);
        let end = beat_from_content_time(
            note.start + note.length,
            clip.range.start,
            midi.content_start,
        );
        if runtime.playhead >= start && runtime.playhead < end {
            pitches.insert(note.pitch);
        }
    }
    pitches
}

pub(super) fn preview_note_for_action(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &MidiClipData,
    track_id: TrackId,
    note_id: NoteId,
    tempo_bpm: f64,
    now: f64,
) {
    let Some(note) = midi.notes.iter().find(|note| note.id == note_id) else {
        return;
    };
    state.preview_highlights.insert(
        note.pitch,
        now + preview_highlight_seconds(note.length, tempo_bpm),
    );
    ui.emit(UiIntent::PreviewNote {
        track_id,
        note_id,
        pitch: note.pitch,
        velocity: note.velocity.0,
        length_mus: Some(capped_preview_length(note.length, tempo_bpm)),
    });
}
