use std::collections::BTreeMap;

use egui::{Rect, pos2};
use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::{Id, Meta, ProjectDoc, SampleRate, TrackId, apply};
use notwig_ui::DropAudioTarget;
use notwig_ui::test_api::drop_target::resolve_drop_target;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Drop".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let mut doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(2),
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(3),
            chain_id: id::<DeviceChain>(103),
            kind: TrackKind::Instrument,
            name: "Instrument".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    doc
}

fn rows() -> BTreeMap<TrackId, Rect> {
    BTreeMap::from([
        (
            id::<Track>(1),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 48.0)),
        ),
        (
            id::<Track>(2),
            Rect::from_min_max(pos2(0.0, 48.0), pos2(800.0, 96.0)),
        ),
        (
            id::<Track>(3),
            Rect::from_min_max(pos2(0.0, 96.0), pos2(800.0, 144.0)),
        ),
    ])
}

#[test]
fn audio_lane_targets_existing_track() {
    let doc = doc();
    assert_eq!(
        resolve_drop_target(&doc, &rows(), 72.0),
        Some(DropAudioTarget::Track(id::<Track>(2)))
    );
}

#[test]
fn boundary_creates_track_at_boundary() {
    let doc = doc();
    assert_eq!(
        resolve_drop_target(&doc, &rows(), 96.0),
        Some(DropAudioTarget::NewTrack { index: 2 })
    );
    assert_eq!(
        resolve_drop_target(&doc, &rows(), 49.0),
        Some(DropAudioTarget::NewTrack { index: 1 })
    );
}

#[test]
fn non_audio_lane_creates_track_after_it() {
    let doc = doc();
    assert_eq!(
        resolve_drop_target(&doc, &rows(), 120.0),
        Some(DropAudioTarget::NewTrack { index: 3 })
    );
}

#[test]
fn below_rows_creates_track_at_end() {
    let doc = doc();
    assert_eq!(
        resolve_drop_target(&doc, &rows(), 170.0),
        Some(DropAudioTarget::NewTrack { index: 3 })
    );
}
