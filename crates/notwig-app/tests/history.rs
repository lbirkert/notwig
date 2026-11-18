use notwig_app::history::History;
use notwig_core::ids::{DeviceChain, Track};
use notwig_core::{Id, Meta, ProjectDoc, SampleRate};
use uuid::Uuid;

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "History".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(
        meta,
        Id::<Track>::from_uuid(Uuid::from_u128(1)),
        Id::<DeviceChain>::from_uuid(Uuid::from_u128(2)),
    )
    .unwrap()
}

#[test]
fn undo_and_redo_roundtrip() {
    let mut history = History::default();
    let mut current = doc();
    let before = current.clone();
    current.revision = 1;
    history.record(before.clone(), false);
    let previous = history.undo(&current).unwrap();
    assert_eq!(previous.doc.revision, before.revision);
    assert!(!previous.structural);
    let next = history.redo(&previous.doc).unwrap();
    assert_eq!(next.doc.revision, 1);
    assert!(!next.structural);
}

#[test]
fn structural_flag_survives_undo_and_redo() {
    let mut history = History::default();
    let mut current = doc();
    let before = current.clone();
    current.revision = 1;
    history.record(before, true);
    let previous = history.undo(&current).unwrap();
    assert!(previous.structural);
    let next = history.redo(&previous.doc).unwrap();
    assert!(next.structural);
}

#[test]
fn recording_clears_redo() {
    let mut history = History::default();
    let current = doc();
    history.record(current.clone(), false);
    let previous = history.undo(&current).unwrap();
    history.record(previous.doc, false);
    assert_eq!(history.redo_len(), 0);
}
