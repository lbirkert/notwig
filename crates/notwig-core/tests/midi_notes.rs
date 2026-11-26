use std::collections::BTreeMap;

use notwig_core::ids::{Asset, Clip, DeviceChain as DeviceChainMarker, Note, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{
    AudioClipData, ClipKind, ClipPayload, ExprValue, FadeShape, MidiClipData, NoteEntity,
};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn note(n: u128, pitch: u8, start: i64, len: i64) -> NoteEntity {
    NoteEntity {
        id: id::<Note>(n),
        pitch,
        start: Rational::new(start, 1),
        length: Rational::new(len, 1),
        velocity: Norm01::clamped(0.8),
        release_velocity: None,
        expression: BTreeMap::<String, ExprValue>::new(),
    }
}

fn meta() -> Meta {
    Meta {
        name: "Notes".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn midi_doc(notes: Vec<NoteEntity>) -> (ProjectDoc, ClipId) {
    let mut doc =
        ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChainMarker>(100)).unwrap();
    let track = id::<Track>(2);
    let chain = id::<DeviceChainMarker>(102);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: "Keys".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let clip = id::<Clip>(20);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(4, 1),
                content_start: Rational::zero(),
                notes,
            }),
        }),
    )
    .unwrap()
    .doc;
    (doc, clip)
}

fn audio_doc() -> (ProjectDoc, ClipId) {
    let mut doc =
        ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChainMarker>(100)).unwrap();
    let track = id::<Track>(2);
    let chain = id::<DeviceChainMarker>(102);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let asset = id::<Asset>(30);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: "assets/a.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap()
    .doc;
    let clip = id::<Clip>(21);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 24_000,
                gain_db: Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        }),
    )
    .unwrap()
    .doc;
    (doc, clip)
}

fn notes_of(doc: &ProjectDoc, clip: ClipId) -> &[NoteEntity] {
    match &doc.arranger.clips[&clip].payload {
        ClipPayload::Midi(midi) => &midi.notes,
        ClipPayload::Audio(_) => panic!("expected MIDI clip"),
    }
}

fn run(doc: &ProjectDoc, payload: CommandPayload) -> Result<ProjectDoc, CoreError> {
    apply(doc, &Command::new(payload)).map(|result| result.doc)
}

#[test]
fn create_note_keeps_overlaps_and_sorts_by_start_pitch_and_id() {
    let (doc, clip) = midi_doc(Vec::new());
    let doc = run(
        &doc,
        CommandPayload::CreateNote {
            clip_id: clip,
            note: note(5, 64, 1, 1),
        },
    )
    .unwrap();
    let doc = run(
        &doc,
        CommandPayload::CreateNote {
            clip_id: clip,
            note: note(3, 60, 1, 2),
        },
    )
    .unwrap();
    let doc = run(
        &doc,
        CommandPayload::CreateNote {
            clip_id: clip,
            note: note(4, 60, 0, 1),
        },
    )
    .unwrap();
    let notes = notes_of(&doc, clip);
    let ids: Vec<NoteId> = notes.iter().map(|note| note.id).collect();
    assert_eq!(ids, vec![id::<Note>(4), id::<Note>(3), id::<Note>(5)]);
    assert_eq!(notes[1].start, notes[2].start);
    assert_eq!(notes[1].pitch, 60);
}

#[test]
fn create_note_rejects_duplicate_ids() {
    let (doc, clip) = midi_doc(vec![note(1, 60, 0, 1)]);
    let result = run(
        &doc,
        CommandPayload::CreateNote {
            clip_id: clip,
            note: note(1, 62, 1, 1),
        },
    );
    assert!(matches!(result, Err(CoreError::DuplicateId { .. })));
}

#[test]
fn create_note_validates_bounds_and_pitch() {
    let (doc, clip) = midi_doc(Vec::new());
    for bad in [note(1, 128, 0, 1), note(1, 60, 0, 0)] {
        let result = run(
            &doc,
            CommandPayload::CreateNote {
                clip_id: clip,
                note: bad,
            },
        );
        assert!(
            matches!(result, Err(CoreError::ValidationFailed { .. })),
            "expected validation failure"
        );
    }
}

#[test]
fn create_note_allows_notes_outside_the_midi_content_bounds() {
    let (doc, clip) = midi_doc(Vec::new());
    for allowed in [note(1, 60, -2, 1), note(2, 60, 8, 2)] {
        let result = run(
            &doc,
            CommandPayload::CreateNote {
                clip_id: clip,
                note: allowed,
            },
        );
        assert!(result.is_ok(), "notes outside content should be editable");
    }
}

#[test]
fn note_commands_reject_audio_clips() {
    let (doc, clip) = audio_doc();
    let result = run(
        &doc,
        CommandPayload::CreateNote {
            clip_id: clip,
            note: note(1, 60, 0, 1),
        },
    );
    assert!(matches!(result, Err(CoreError::ValidationFailed { .. })));
}

#[test]
fn set_notes_upserts_updates_and_inserts() {
    let (doc, clip) = midi_doc(vec![note(1, 60, 0, 1), note(2, 62, 2, 1)]);
    let updated = note(2, 67, 1, 2);
    let created = note(3, 64, 1, 1);
    let doc = run(
        &doc,
        CommandPayload::SetNotes {
            clip_id: clip,
            notes: vec![updated.clone(), created.clone()],
        },
    )
    .unwrap();
    let notes = notes_of(&doc, clip);
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0].id, id::<Note>(1));
    assert_eq!(notes[1].start, Rational::new(1, 1));
    assert_eq!(notes[1].pitch, 64);
    assert_eq!(notes[2].pitch, 67);
    assert_eq!(notes[2].length, Rational::new(2, 1));
}

#[test]
fn set_notes_rejects_duplicate_ids_in_the_batch() {
    let (doc, clip) = midi_doc(Vec::new());
    let result = run(
        &doc,
        CommandPayload::SetNotes {
            clip_id: clip,
            notes: vec![note(1, 60, 0, 1), note(1, 64, 1, 1)],
        },
    );
    assert!(matches!(result, Err(CoreError::DuplicateId { .. })));
}

#[test]
fn set_notes_validates_every_note_before_committing() {
    let (doc, clip) = midi_doc(vec![note(1, 60, 0, 1)]);
    let result = run(
        &doc,
        CommandPayload::SetNotes {
            clip_id: clip,
            notes: vec![note(1, 62, 1, 1), note(2, 128, 3, 2)],
        },
    );
    assert!(matches!(result, Err(CoreError::ValidationFailed { .. })));
    assert_eq!(notes_of(&doc, clip)[0].pitch, 60);
}

#[test]
fn delete_notes_removes_multiple_and_rejects_missing_ids() {
    let (doc, clip) = midi_doc(vec![
        note(1, 60, 0, 1),
        note(2, 62, 1, 1),
        note(3, 64, 2, 1),
    ]);
    let doc = run(
        &doc,
        CommandPayload::DeleteNotes {
            clip_id: clip,
            note_ids: vec![id::<Note>(1), id::<Note>(3)],
        },
    )
    .unwrap();
    let notes = notes_of(&doc, clip);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, id::<Note>(2));

    let result = run(
        &doc,
        CommandPayload::DeleteNotes {
            clip_id: clip,
            note_ids: vec![id::<Note>(2), id::<Note>(99)],
        },
    );
    assert!(matches!(result, Err(CoreError::NotFound { .. })));
    assert_eq!(notes_of(&doc, clip).len(), 1);
}

#[test]
fn legacy_set_note_and_delete_note_still_work() {
    let (doc, clip) = midi_doc(Vec::new());
    let doc = run(
        &doc,
        CommandPayload::SetNote {
            clip_id: clip,
            note: note(1, 60, 0, 2),
        },
    )
    .unwrap();
    let doc = run(
        &doc,
        CommandPayload::SetNote {
            clip_id: clip,
            note: note(1, 62, 1, 1),
        },
    )
    .unwrap();
    assert_eq!(notes_of(&doc, clip).len(), 1);
    assert_eq!(notes_of(&doc, clip)[0].pitch, 62);
    let doc = run(
        &doc,
        CommandPayload::DeleteNote {
            clip_id: clip,
            note_id: id::<Note>(1),
        },
    )
    .unwrap();
    assert_eq!(notes_of(&doc, clip).len(), 0);
}
