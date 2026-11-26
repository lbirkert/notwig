use std::collections::BTreeMap;

use notwig_core::apply::create_track;
use notwig_core::ids::{Asset, Clip, Device, DeviceChain as DeviceChainMarker, Note, Track};
use notwig_core::protocol::{AudioEdit, Command, CommandPayload, EngineMsg, StateDelta};
use notwig_core::sequencer::{
    AudioClipData, ClipKind, ClipPayload, ExprValue, FadeShape, MidiClipData, NoteEntity,
};
use notwig_core::tracks::{TrackKind, TrackOutput};
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
        name: "Test".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn new_doc() -> ProjectDoc {
    ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChainMarker>(100)).unwrap()
}

fn add_track(doc: &mut ProjectDoc, n: u128, kind: TrackKind) -> (TrackId, DeviceChainId) {
    let track = id::<Track>(n);
    let chain = id::<DeviceChainMarker>(n + 1000);
    let result = apply(
        doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind,
            name: format!("Track {n}"),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap();
    *doc = result.doc;
    (track, chain)
}

fn create_midi_clip(
    doc: &ProjectDoc,
    track: TrackId,
    clip_n: u128,
    range: RangeMus,
    notes: Vec<NoteEntity>,
) -> ProjectDoc {
    apply(
        doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(clip_n),
            track_id: track,
            kind: ClipKind::Midi,
            range,
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(4, 1),
                content_start: Rational::zero(),
                notes,
            }),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn template_has_one_master_and_validates() {
    let doc = new_doc();
    assert_eq!(doc.tracks.len(), 1);
    assert!(doc.master_track().is_some());
    doc.validate().unwrap();
}

#[test]
fn create_track_validates_hierarchy_and_duplicate_ids() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    assert!(doc.tracks.contains_key(&track));

    let duplicate = Command::new(CommandPayload::CreateTrack {
        id: track,
        chain_id: id::<DeviceChainMarker>(500),
        kind: TrackKind::Audio,
        name: "dup".into(),
        parent: None,
        index: None,
        color: None,
    });
    assert!(matches!(
        apply(&doc, &duplicate),
        Err(CoreError::DuplicateId { .. })
    ));

    let root = doc.track_order.roots[0];
    let bad_parent = Command::new(CommandPayload::CreateTrack {
        id: id::<Track>(9),
        chain_id: id::<DeviceChainMarker>(509),
        kind: TrackKind::Audio,
        name: "bad".into(),
        parent: Some(root),
        index: None,
        color: None,
    });
    assert!(matches!(
        apply(&doc, &bad_parent),
        Err(CoreError::ValidationFailed { .. })
    ));
}

#[test]
fn group_delete_removes_descendants_clips_and_devices() {
    let doc = new_doc();
    let group_id = id::<Track>(10);
    let group_chain = id::<DeviceChainMarker>(1010);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: group_id,
            chain_id: group_chain,
            kind: TrackKind::Group,
            name: "Group".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let child = id::<Track>(11);
    let child_chain = id::<DeviceChainMarker>(1011);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: child,
            chain_id: child_chain,
            kind: TrackKind::Instrument,
            name: "Child".into(),
            parent: Some(group_id),
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;

    let doc = create_midi_clip(
        &doc,
        child,
        20,
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        vec![note(30, 60, 0, 1)],
    );

    let result = apply(
        &doc,
        &Command::new(CommandPayload::DeleteTrack { id: group_id }),
    )
    .unwrap();
    assert!(!result.doc.tracks.contains_key(&group_id));
    assert!(!result.doc.tracks.contains_key(&child));
    assert!(result.doc.arranger.clips.is_empty());
    assert!(!result.doc.chains.contains_key(&child_chain));
}

#[test]
fn overlap_makes_space_and_preserves_both_fragments() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Instrument);
    let doc = create_midi_clip(
        &doc,
        track,
        20,
        RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
        vec![],
    );
    let doc = create_midi_clip(
        &doc,
        track,
        21,
        RangeMus::new(Rational::new(2, 1), Rational::new(3, 1)).unwrap(),
        vec![],
    );

    let lane = &doc.arranger.lanes[&track];
    assert_eq!(lane.clips.len(), 3);
    let first = &doc.arranger.clips[&id::<Clip>(20)];
    assert_eq!(first.range.start, Rational::zero());
    assert_eq!(first.range.end, Rational::new(2, 1));
    let second = &doc.arranger.clips[&id::<Clip>(21)];
    assert_eq!(second.range.start, Rational::new(2, 1));
    assert_eq!(second.range.end, Rational::new(3, 1));
    assert!(doc.arranger.clips.values().any(|clip| {
        clip.id != id::<Clip>(20)
            && clip.id != id::<Clip>(21)
            && clip.range.start == Rational::new(3, 1)
            && clip.range.end == Rational::new(4, 1)
    }));
}

#[test]
fn split_crossing_note_uses_supplied_ids() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Instrument);
    let clip = id::<Clip>(20);
    let doc = create_midi_clip(
        &doc,
        track,
        clip.into_uuid().as_u128(),
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        vec![note(30, 60, 0, 4)],
    );

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SplitClip {
            clip_id: clip,
            at: Rational::new(1, 1),
            new_clip_id: id::<Clip>(21),
            new_note_ids: vec![id::<Note>(31)],
        }),
    )
    .unwrap();
    let left = &result.doc.arranger.clips[&clip];
    let right = &result.doc.arranger.clips[&id::<Clip>(21)];
    let ClipPayload::Midi(left_midi) = &left.payload else {
        panic!()
    };
    let ClipPayload::Midi(right_midi) = &right.payload else {
        panic!()
    };
    assert_eq!(left_midi.notes[0].length, Rational::new(1, 1));
    assert_eq!(right_midi.notes[0].id, id::<Note>(31));
    assert_eq!(right_midi.notes[0].length, Rational::new(3, 1));
    assert_eq!(right.range.start, Rational::new(1, 1));
}

#[test]
fn transport_stop_returns_to_loop_start_and_emits_engine_msg() {
    let doc = new_doc();
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::SetTransport {
            playing: Some(true),
            playhead: Some(Rational::new(5, 1)),
            loop_enabled: Some(true),
            loop_range: Some(Some(
                RangeMus::new(Rational::new(2, 1), Rational::new(6, 1)).unwrap(),
            )),
            metronome: None,
        }),
    )
    .unwrap()
    .doc;

    let result = apply(&doc, &Command::new(CommandPayload::Stop)).unwrap();
    assert!(!result.doc.transport.playing);
    assert_eq!(result.doc.transport.playhead, Rational::new(2, 1));
    assert!(
        result
            .engine_msgs
            .iter()
            .any(|msg| matches!(msg, EngineMsg::AllNotesOff))
    );
    assert!(
        result
            .engine_msgs
            .iter()
            .any(|msg| matches!(msg, EngineMsg::SetTransport { playing: false, .. }))
    );
}

#[test]
fn tempo_change_recomputes_loop_samples_from_musical_range() {
    let doc = new_doc();
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::SetTransport {
            playing: Some(true),
            playhead: Some(Rational::new(1, 1)),
            loop_enabled: Some(true),
            loop_range: Some(Some(
                RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            )),
            metronome: None,
        }),
    )
    .unwrap()
    .doc;

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTempo { tempo_bpm: 60.0 }),
    )
    .unwrap();

    assert!(
        result
            .engine_msgs
            .iter()
            .any(|msg| matches!(msg, EngineMsg::UpdateSchedule { .. })),
        "tempo change must still refresh the playback schedule"
    );

    let (playhead_samples, loop_start_samples, loop_end_samples) = result
        .engine_msgs
        .iter()
        .find_map(|msg| match msg {
            EngineMsg::SetTransport {
                playhead_samples,
                loop_start_samples,
                loop_end_samples,
                ..
            } => Some((*playhead_samples, *loop_start_samples, *loop_end_samples)),
            _ => None,
        })
        .expect("tempo change must emit a transport update");

    // Four beats at 60 BPM and a 48 kHz sample rate is 192_000 samples.
    assert_eq!(loop_start_samples, Some(0));
    assert_eq!(loop_end_samples, Some(192_000));
    // One beat at 60 BPM is 48_000 samples.
    assert_eq!(playhead_samples, 48_000);
}

#[test]
fn audio_clip_requires_sample_accurate_range() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = id::<Asset>(500);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: "assets/sha256.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(90_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap()
    .doc;

    let good = Command::new(CommandPayload::CreateClip {
        id: id::<Clip>(20),
        track_id: track,
        kind: ClipKind::Audio,
        range: RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap(),
        payload: ClipPayload::Audio(AudioClipData {
            asset_id: asset,
            source_start_sample: 0,
            source_end_sample: 24_000,
            gain_db: Db(0.0),
            gain_curve: None,
            fade_in_samples: 100,
            fade_out_samples: 100,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        }),
    });
    let doc = apply(&doc, &good).unwrap().doc;

    let bad = Command::new(CommandPayload::CreateClip {
        id: id::<Clip>(21),
        track_id: track,
        kind: ClipKind::Audio,
        range: RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap(),
        payload: ClipPayload::Audio(AudioClipData {
            asset_id: asset,
            source_start_sample: 0,
            source_end_sample: 12_000,
            gain_db: Db(0.0),
            gain_curve: None,
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        }),
    });
    assert!(matches!(
        apply(&doc, &bad),
        Err(CoreError::ValidationFailed { .. })
    ));
}

#[test]
fn edit_audio_clip_updates_gain_and_range() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let asset = id::<Asset>(500);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: "assets/x.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(96_000),
                original_sample_rate: None,
                original_channels: None,
            },
        }),
    )
    .unwrap()
    .doc;
    let clip = id::<Clip>(20);
    let doc = apply(
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

    let result = apply(
        &doc,
        &Command::new(CommandPayload::EditClip {
            clip_id: clip,
            range: None,
            name: None,
            muted: None,
            content_start: None,
            content_length: None,
            audio: Some(AudioEdit {
                source_start_sample: None,
                source_end_sample: None,
                gain_db: Some(Db(-6.0)),
                fade_in_samples: None,
                fade_out_samples: None,
            }),
        }),
    )
    .unwrap();
    let ClipPayload::Audio(data) = &result.doc.arranger.clips[&clip].payload else {
        panic!()
    };
    assert_eq!(data.gain_db, Db(-6.0));
}

#[test]
fn device_placement_rules_are_enforced() {
    let mut doc = new_doc();
    let (_track, chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    let device = id::<Device>(300);
    let mut doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: device,
            type_id: "notwig.tool".into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;
    assert!(doc.devices.contains_key(&device));

    let second = Command::new(CommandPayload::InsertDevice {
        id: id::<Device>(301),
        type_id: "notwig.tool".into(),
        version: None,
        kind: DeviceKind::Instrument,
        target_chain: chain,
        index: None,
        child_chain_ids: vec![],
        name: None,
    });
    assert!(matches!(
        apply(&doc, &second),
        Err(CoreError::ValidationFailed { .. })
    ));

    let (audio_track, audio_chain) = add_track(&mut doc, 3, TrackKind::Audio);
    let note_fx = Command::new(CommandPayload::InsertDevice {
        id: id::<Device>(302),
        type_id: "notwig.note".into(),
        version: None,
        kind: DeviceKind::NoteFx,
        target_chain: audio_chain,
        index: None,
        child_chain_ids: vec![],
        name: None,
    });
    assert!(matches!(
        apply(&doc, &note_fx),
        Err(CoreError::ValidationFailed { .. })
    ));
    let _ = audio_track;
}

#[test]
fn move_device_reorders_within_a_chain() {
    let mut doc = new_doc();
    let (_track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    for n in 300..303 {
        doc = apply(
            &doc,
            &Command::new(CommandPayload::InsertDevice {
                id: id::<Device>(n),
                type_id: "notwig.tool".into(),
                version: None,
                kind: DeviceKind::AudioFx,
                target_chain: chain,
                index: None,
                child_chain_ids: vec![],
                name: None,
            }),
        )
        .unwrap()
        .doc;
    }
    assert_eq!(
        doc.chains[&chain].devices,
        vec![id::<Device>(300), id::<Device>(301), id::<Device>(302)]
    );
    doc = apply(
        &doc,
        &Command::new(CommandPayload::MoveDevice {
            device_id: id::<Device>(302),
            to_chain: chain,
            index: 0,
        }),
    )
    .unwrap()
    .doc;
    assert_eq!(
        doc.chains[&chain].devices,
        vec![id::<Device>(302), id::<Device>(300), id::<Device>(301)]
    );
    doc = apply(
        &doc,
        &Command::new(CommandPayload::MoveDevice {
            device_id: id::<Device>(302),
            to_chain: chain,
            index: 3,
        }),
    )
    .unwrap()
    .doc;
    assert_eq!(
        doc.chains[&chain].devices,
        vec![id::<Device>(300), id::<Device>(301), id::<Device>(302)]
    );
}

#[test]
fn move_device_crosses_chains_and_enforces_track_placement() {
    let mut doc = new_doc();
    let (_instrument_track, instrument_chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    let (_audio_track, audio_chain) = add_track(&mut doc, 3, TrackKind::Audio);
    let instrument = id::<Device>(300);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: instrument,
            type_id: "notwig.synth".into(),
            version: None,
            kind: DeviceKind::Instrument,
            target_chain: instrument_chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;

    assert!(matches!(
        apply(
            &doc,
            &Command::new(CommandPayload::MoveDevice {
                device_id: instrument,
                to_chain: audio_chain,
                index: 0,
            })
        ),
        Err(CoreError::ValidationFailed { .. })
    ));
    doc = apply(
        &doc,
        &Command::new(CommandPayload::MoveDevice {
            device_id: instrument,
            to_chain: instrument_chain,
            index: 0,
        }),
    )
    .unwrap()
    .doc;
    assert_eq!(doc.chains[&instrument_chain].devices, vec![instrument]);

    let fx = id::<Device>(301);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: fx,
            type_id: "notwig.tool".into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: instrument_chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;
    doc = apply(
        &doc,
        &Command::new(CommandPayload::MoveDevice {
            device_id: fx,
            to_chain: audio_chain,
            index: 0,
        }),
    )
    .unwrap()
    .doc;
    assert_eq!(doc.chains[&audio_chain].devices, vec![fx]);
}

#[test]
fn move_device_rejects_its_own_child_chain() {
    let mut doc = new_doc();
    let (_track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    let container = id::<Device>(300);
    let child_chain = id::<DeviceChainMarker>(400);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: container,
            type_id: notwig_core::dsp::stock::CHAIN_TYPE_ID.into(),
            version: None,
            kind: DeviceKind::Container,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![child_chain],
            name: None,
        }),
    )
    .unwrap()
    .doc;
    assert!(matches!(
        apply(
            &doc,
            &Command::new(CommandPayload::MoveDevice {
                device_id: container,
                to_chain: child_chain,
                index: 0,
            })
        ),
        Err(CoreError::ValidationFailed { .. })
    ));
}

#[test]
fn routing_cycles_are_rejected() {
    let mut doc = new_doc();
    let (a, _) = add_track(&mut doc, 2, TrackKind::Audio);
    let (b, _) = add_track(&mut doc, 3, TrackKind::Audio);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::SetTrackRoute {
            id: a,
            audio_out: TrackOutput::Track(b),
        }),
    )
    .unwrap()
    .doc;
    let cycle = Command::new(CommandPayload::SetTrackRoute {
        id: b,
        audio_out: TrackOutput::Track(a),
    });
    assert!(matches!(
        apply(&doc, &cycle),
        Err(CoreError::ValidationFailed { .. })
    ));
}

#[test]
fn project_json_roundtrip_preserves_document() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Instrument);
    let doc = create_midi_clip(
        &doc,
        track,
        20,
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        vec![note(30, 61, 0, 1)],
    );
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let back: ProjectDoc = serde_json::from_str(&json).unwrap();
    assert_eq!(doc, back);
    assert!(json.contains("notes"));
}

#[test]
fn state_delta_reports_affected_entities() {
    let mut doc = new_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    let result = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(4),
            chain_id: id::<DeviceChainMarker>(1004),
            kind: TrackKind::Audio,
            name: "A".into(),
            parent: None,
            index: Some(0),
            color: None,
        }),
    )
    .unwrap();
    let _: StateDelta = result.delta.clone();
    assert!(result.delta.affected.tracks.contains(&id::<Track>(4)));
    assert_eq!(result.doc.revision, 2);
    let _ = (track, chain);
}

#[test]
fn duplicate_clip_maps_note_ids() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Instrument);
    let clip = id::<Clip>(20);
    let doc = create_midi_clip(
        &doc,
        track,
        clip.into_uuid().as_u128(),
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        vec![note(30, 60, 0, 1)],
    );
    let result = apply(
        &doc,
        &Command::new(CommandPayload::DuplicateClip {
            clip_id: clip,
            new_clip_id: id::<Clip>(21),
            note_id_map: vec![(id::<Note>(30), id::<Note>(31))],
            track_id: None,
            range: None,
        }),
    )
    .unwrap();
    let new_clip = &result.doc.arranger.clips[&id::<Clip>(21)];
    assert_eq!(new_clip.range.start, Rational::new(2, 1));
    let ClipPayload::Midi(midi) = &new_clip.payload else {
        panic!()
    };
    assert_eq!(midi.notes[0].id, id::<Note>(31));
}

#[test]
fn engine_schedule_contains_playable_clips() {
    let mut doc = new_doc();
    let (track, _) = add_track(&mut doc, 2, TrackKind::Instrument);
    let doc = create_midi_clip(
        &doc,
        track,
        20,
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        vec![note(30, 60, 0, 1)],
    );
    let schedule = PlaybackSchedule::from_doc(&doc);
    assert_eq!(schedule.midi_clips(track).count(), 1);
}

#[test]
fn create_track_helper_is_usable_directly() {
    let mut doc = new_doc();
    create_track(
        &mut doc,
        id::<Track>(77),
        id::<DeviceChainMarker>(1077),
        TrackKind::BusFx,
        "Bus".into(),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(doc.tracks.contains_key(&id::<Track>(77)));
}

#[test]
fn instrument_track_adopts_named_instrument() {
    let mut doc = new_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    assert_eq!(doc.tracks[&track].name, "Track 2");
    assert!(doc.tracks[&track].name_is_auto);

    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(300),
            type_id: "test.operator".into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: Some("Operator".into()),
        }),
    )
    .unwrap()
    .doc;

    assert_eq!(doc.tracks[&track].name, "Operator");
    assert!(doc.tracks[&track].name_is_auto);
}

#[test]
fn renamed_instrument_track_keeps_its_name_on_insert() {
    let mut doc = new_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Instrument);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::RenameTrack {
            id: track,
            name: "Keys".into(),
        }),
    )
    .unwrap()
    .doc;
    assert!(!doc.tracks[&track].name_is_auto);

    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(300),
            type_id: "test.operator".into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::Instrument,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: Some("Operator".into()),
        }),
    )
    .unwrap()
    .doc;

    assert_eq!(doc.tracks[&track].name, "Keys");
    assert!(!doc.tracks[&track].name_is_auto);
}

#[test]
fn named_devices_do_not_rename_non_instrument_tracks() {
    let mut doc = new_doc();
    let (track, chain) = add_track(&mut doc, 2, TrackKind::Audio);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(300),
            type_id: "test.eq".into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: vec![],
            name: Some("Equalizer".into()),
        }),
    )
    .unwrap()
    .doc;

    assert_eq!(doc.tracks[&track].name, "Track 2");
    assert!(doc.tracks[&track].name_is_auto);
}

#[test]
fn old_serialized_track_loads_without_automatic_name() {
    let mut doc = new_doc();
    add_track(&mut doc, 2, TrackKind::Instrument);
    let mut value = serde_json::to_value(&doc).unwrap();
    for track in value["tracks"].as_object_mut().unwrap().values_mut() {
        track
            .as_object_mut()
            .unwrap()
            .remove("name_is_auto")
            .expect("serialized track carries name_is_auto");
    }
    let back: ProjectDoc = serde_json::from_value(value).unwrap();
    assert!(back.tracks.values().all(|track| !track.name_is_auto));
}
