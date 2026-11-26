use notwig_app::audio::AudioSnapshot;
use notwig_app::workspace::Workspace;
use notwig_core::protocol::{CommandPayload, EngineMsg};
use notwig_core::{RtEvent, TimeMus, TrackId};
use notwig_ui::MeterValues;
use uuid::Uuid;

fn drain_builds(workspace: &mut Workspace) {
    while workspace.engine_building() {
        workspace.poll_engine_build();
        std::thread::yield_now();
    }
}

fn tick_after_processing(workspace: &mut Workspace, playhead_samples: u64) -> TimeMus {
    let mut engine = workspace.offline_engine().unwrap();
    engine
        .apply(EngineMsg::SetTransport {
            playing: true,
            playhead_samples,
            loop_enabled: false,
            loop_start_samples: None,
            loop_end_samples: None,
            stop: false,
            metronome: None,
        })
        .unwrap();
    let mut output = vec![0.0_f32; 128 * 2];
    engine.process(&mut output, 128).unwrap();
    let mut events = Vec::new();
    engine.drain_events(&mut events);
    events
        .iter()
        .find_map(|event| match event {
            RtEvent::TransportTick { playhead_mus, .. } => Some(*playhead_mus),
            _ => None,
        })
        .expect("transport tick")
}

#[test]
fn tempo_change_reaches_the_engine_plan() {
    let mut workspace = Workspace::new_empty().unwrap();
    drain_builds(&mut workspace);
    workspace
        .apply(CommandPayload::SetTempo { tempo_bpm: 60.0 })
        .unwrap();
    assert_eq!(workspace.doc.meta.tempo_bpm, 60.0);
    drain_builds(&mut workspace);

    // 48_000 samples is one beat at 60 BPM but two beats at the old 120 BPM.
    let tick = tick_after_processing(&mut workspace, 48_000);
    let expected_beats = 48_128.0 / 48_000.0;
    assert!(
        (tick.as_f64() - expected_beats).abs() < 1e-3,
        "expected about {expected_beats} beats at 60 BPM, got {}",
        tick.as_f64()
    );
}

#[test]
fn stopped_transport_hides_stale_meters() {
    let track = TrackId::from_uuid(Uuid::from_u128(9));
    let mut snapshot = AudioSnapshot::default();
    snapshot.running = true;
    snapshot.meters.insert(
        track,
        MeterValues {
            peak_l: 0.5,
            peak_r: 0.25,
            rms_l: 0.2,
            rms_r: 0.1,
        },
    );

    assert!(snapshot.meters_for_display(true).contains_key(&track));
    assert!(snapshot.meters_for_display(false).is_empty());
}

#[test]
fn tempo_change_succeeds_with_an_audio_clip() {
    let mut workspace = Workspace::new_empty().unwrap();
    workspace
        .create_track(notwig_core::TrackKind::Audio)
        .unwrap();
    let track = workspace
        .doc
        .tracks
        .values()
        .find(|track| track.kind == notwig_core::TrackKind::Audio)
        .unwrap()
        .id;
    let asset = notwig_core::Id::<notwig_core::ids::Asset>::from_uuid(Uuid::from_u128(1));
    workspace
        .apply(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: notwig_core::AssetEntry {
                id: asset,
                kind: notwig_core::AssetKind::Audio,
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
        })
        .unwrap();
    workspace
        .apply(CommandPayload::CreateClip {
            id: notwig_core::Id::<notwig_core::ids::Clip>::from_uuid(Uuid::from_u128(2)),
            track_id: track,
            kind: notwig_core::ClipKind::Audio,
            range: notwig_core::RangeMus::new(
                notwig_core::Rational::zero(),
                notwig_core::Rational::new(2, 1),
            )
            .unwrap(),
            payload: notwig_core::ClipPayload::Audio(notwig_core::AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: notwig_core::Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: notwig_core::sequencer::FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        })
        .unwrap();
    workspace
        .apply(CommandPayload::SetTempo { tempo_bpm: 90.0 })
        .expect("tempo change should not be blocked by sample-locked audio clips");
    assert_eq!(workspace.doc.meta.tempo_bpm, 90.0);
    let clip = workspace
        .doc
        .arranger
        .clips
        .values()
        .find(|clip| clip.track_id == track)
        .expect("audio clip");
    // 48_000 samples is 2 beats at 120 BPM but 1.5 beats at 90 BPM.
    assert_eq!(
        clip.range,
        notwig_core::RangeMus::new(
            notwig_core::Rational::zero(),
            notwig_core::Rational::new(3, 2),
        )
        .unwrap()
    );
}
