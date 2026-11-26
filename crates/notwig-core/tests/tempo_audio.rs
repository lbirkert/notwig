use notwig_core::ids::{Asset, Clip, DeviceChain as DeviceChainMarker, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Tempo".into(),
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

fn audio_doc() -> (ProjectDoc, TrackId, AssetId) {
    let doc = new_doc();
    let track = id::<Track>(2);
    let asset = id::<Asset>(500);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: id::<DeviceChainMarker>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: "assets/tone.wav".into(),
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
    (doc, track, asset)
}

fn add_clip(
    doc: &ProjectDoc,
    track: TrackId,
    asset: AssetId,
    n: u128,
    range: RangeMus,
    source_start: u64,
    source_end: u64,
) -> ProjectDoc {
    apply(
        doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(n),
            track_id: track,
            kind: ClipKind::Audio,
            range,
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: source_start,
                source_end_sample: source_end,
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
    .doc
}

fn assert_lane_has_no_overlaps(doc: &ProjectDoc, track: TrackId) {
    let lane = &doc.arranger.lanes[&track];
    for pair in lane.clips.windows(2) {
        let left = &doc.arranger.clips[&pair[0]];
        let right = &doc.arranger.clips[&pair[1]];
        assert!(
            left.range.end <= right.range.start,
            "clips {:?} and {:?} overlap: {:?} / {:?}",
            left.id,
            right.id,
            left.range,
            right.range
        );
    }
}

#[test]
fn faster_tempo_trims_the_later_adjacent_clip_instead_of_overlapping() {
    let (doc, track, asset) = audio_doc();
    let first = RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap();
    let second = RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap();
    let doc = add_clip(&doc, track, asset, 20, first, 0, 24_000);
    let doc = add_clip(&doc, track, asset, 21, second, 0, 24_000);

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTempo { tempo_bpm: 240.0 }),
    )
    .expect("adjacent clips can be resolved by trimming the later start");
    result.doc.validate().unwrap();
    assert_lane_has_no_overlaps(&result.doc, track);

    let left = &result.doc.arranger.clips[&id::<Clip>(20)];
    let right = &result.doc.arranger.clips[&id::<Clip>(21)];
    // 24_000 samples is two beats at 240 BPM.
    assert_eq!(
        left.range,
        RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap()
    );
    // The later clip is trimmed to the earlier clip's end, keeping samples 1
    // beat (12_000 samples) through the end of its source.
    assert_eq!(
        right.range,
        RangeMus::new(Rational::new(2, 1), Rational::new(3, 1)).unwrap()
    );
    let ClipPayload::Audio(audio) = &right.payload else {
        panic!("audio clip")
    };
    assert_eq!(audio.source_start_sample, 12_000);
    assert_eq!(audio.source_end_sample, 24_000);
}

#[test]
fn tempo_change_is_rejected_when_overlap_cannot_be_resolved_safely() {
    let (doc, track, asset) = audio_doc();
    let first = RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap();
    let second = RangeMus::new(Rational::new(1, 1), Rational::new(3, 2)).unwrap();
    let doc = add_clip(&doc, track, asset, 20, first, 0, 24_000);
    // The later clip is short enough that the rescaled first clip would cover
    // it entirely; dropping it would lose content, so the command is rejected.
    let doc = add_clip(&doc, track, asset, 21, second, 0, 12_000);
    let before = doc.clone();

    let result = apply(
        &doc,
        &Command::new(CommandPayload::SetTempo { tempo_bpm: 240.0 }),
    );
    assert!(matches!(result, Err(CoreError::Conflict { .. })));
    assert_eq!(
        doc, before,
        "a rejected tempo change must not mutate its input"
    );
}

#[test]
fn adjacent_audio_rescaling_stays_valid_across_tempos() {
    for bpm in [121.0, 130.0, 150.0, 180.0, 240.0, 300.0, 480.0, 960.0] {
        let (doc, track, asset) = audio_doc();
        let first = RangeMus::new(Rational::zero(), Rational::new(1, 1)).unwrap();
        let second = RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap();
        let doc = add_clip(&doc, track, asset, 20, first, 0, 24_000);
        let doc = add_clip(&doc, track, asset, 21, second, 0, 24_000);

        let result = apply(
            &doc,
            &Command::new(CommandPayload::SetTempo { tempo_bpm: bpm }),
        )
        .unwrap_or_else(|err| panic!("tempo {bpm} should resolve: {err}"));
        result
            .doc
            .validate()
            .unwrap_or_else(|err| panic!("tempo {bpm} produced an invalid document: {err}"));
        assert_lane_has_no_overlaps(&result.doc, track);
    }
}
