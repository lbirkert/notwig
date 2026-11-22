use notwig_app::workspace::Workspace;
use notwig_core::{AudioAssetBuffer, TimeMus, TrackKind};

#[test]
fn finishing_a_recording_registers_a_stereo_asset_and_a_clip() {
    let mut workspace = Workspace::new_empty().expect("workspace");
    let track = workspace.create_track(TrackKind::Audio).expect("track");
    let captured = AudioAssetBuffer {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.25; 48_000],
    };

    let clip_id = workspace
        .finish_recording(track, TimeMus::zero(), captured)
        .expect("recording finalized");
    let clip = workspace
        .doc
        .arranger
        .clips
        .get(&clip_id)
        .expect("recorded clip");
    let notwig_core::ClipPayload::Audio(audio) = &clip.payload else {
        panic!("expected audio clip");
    };
    let asset = workspace
        .doc
        .assets
        .assets
        .get(&audio.asset_id)
        .expect("recorded asset");
    assert_eq!(asset.channels, Some(2));
    assert_eq!(asset.duration_samples, Some(48_000));
    assert_eq!(audio.source_start_sample, 0);
    assert_eq!(audio.source_end_sample, 48_000);
    assert_eq!(workspace.assets[&audio.asset_id].channels, 2);
    assert_eq!(workspace.assets[&audio.asset_id].samples.len(), 96_000);
}
