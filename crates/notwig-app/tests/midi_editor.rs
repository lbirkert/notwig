use std::collections::BTreeMap;

use notwig_app::workspace::Workspace;
use notwig_core::ids::Note as NoteMarker;
use notwig_core::sequencer::NoteEntity;
use notwig_core::{ClipPayload, Id, Norm01, RangeMus, Rational, TrackKind};
use uuid::Uuid;

fn note(id: u128, pitch: u8, start: i64, len: i64) -> NoteEntity {
    NoteEntity {
        id: Id::<NoteMarker>::from_uuid(Uuid::from_u128(id)),
        pitch,
        start: Rational::new(start, 1),
        length: Rational::new(len, 1),
        velocity: Norm01::clamped(0.8),
        release_velocity: None,
        expression: BTreeMap::new(),
    }
}

fn notes(workspace: &Workspace, clip: notwig_core::ClipId) -> &[NoteEntity] {
    match &workspace.doc.arranger.clips[&clip].payload {
        ClipPayload::Midi(midi) => &midi.notes,
        ClipPayload::Audio(_) => panic!("expected MIDI clip"),
    }
}

fn setup() -> (Workspace, notwig_core::ClipId) {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace.create_track(TrackKind::Instrument).unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    workspace
        .create_midi_clip(
            track,
            RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        )
        .unwrap();
    let clip = *workspace.doc.arranger.clips.keys().next().unwrap();
    (workspace, clip)
}

#[test]
fn note_create_edit_and_delete_are_undoable() {
    let (mut workspace, clip) = setup();
    let first = note(1, 60, 0, 1);
    workspace.create_note(clip, first.clone()).unwrap();
    assert_eq!(notes(&workspace, clip).len(), 1);

    let mut moved = first.clone();
    moved.start = Rational::new(1, 1);
    moved.pitch = 64;
    moved.velocity = Norm01::clamped(0.4);
    workspace
        .edit_notes(clip, vec![moved.clone()], Vec::new())
        .unwrap();
    assert_eq!(notes(&workspace, clip)[0].pitch, 64);
    assert_eq!(notes(&workspace, clip)[0].start, Rational::new(1, 1));

    workspace
        .edit_notes(clip, Vec::new(), vec![first.id])
        .unwrap();
    assert_eq!(notes(&workspace, clip).len(), 0);

    workspace.undo().unwrap();
    assert_eq!(notes(&workspace, clip).len(), 1);
    assert_eq!(notes(&workspace, clip)[0].pitch, 64);

    workspace.undo().unwrap();
    assert_eq!(notes(&workspace, clip)[0].pitch, 60);

    workspace.undo().unwrap();
    assert_eq!(notes(&workspace, clip).len(), 0);

    workspace.redo().unwrap();
    assert_eq!(notes(&workspace, clip).len(), 1);
    assert_eq!(notes(&workspace, clip)[0].id, first.id);
}

#[test]
fn one_note_edit_batches_removals_and_updates_into_one_undo_group() {
    let (mut workspace, clip) = setup();
    let first = note(1, 60, 0, 1);
    let second = note(2, 62, 1, 1);
    workspace
        .edit_notes(clip, vec![first.clone(), second.clone()], Vec::new())
        .unwrap();
    assert_eq!(notes(&workspace, clip).len(), 2);

    let mut updated = first.clone();
    updated.pitch = 67;
    workspace
        .edit_notes(clip, vec![updated], vec![second.id])
        .unwrap();
    assert_eq!(notes(&workspace, clip).len(), 1);
    assert_eq!(notes(&workspace, clip)[0].pitch, 67);

    workspace.undo().unwrap();
    assert_eq!(notes(&workspace, clip).len(), 2);
    assert!(notes(&workspace, clip).iter().any(|note| note.pitch == 60));
    assert!(
        notes(&workspace, clip)
            .iter()
            .any(|note| note.id == second.id)
    );

    workspace.redo().unwrap();
    assert_eq!(notes(&workspace, clip).len(), 1);
    assert_eq!(notes(&workspace, clip)[0].pitch, 67);
}

#[test]
fn edited_notes_reach_the_playback_schedule() {
    let (mut workspace, clip) = setup();
    workspace.create_note(clip, note(1, 60, 0, 1)).unwrap();
    let schedule = notwig_core::PlaybackSchedule::from_doc(&workspace.doc);
    let clips: Vec<_> = schedule
        .midi_clips(workspace.doc.arranger.clips[&clip].track_id)
        .collect();
    assert_eq!(clips.len(), 1);
    match &clips[0].payload {
        ClipPayload::Midi(midi) => assert_eq!(midi.notes.len(), 1),
        ClipPayload::Audio(_) => panic!("expected MIDI clip"),
    }
}
