use notwig_core::ids::{Asset, DeviceChain, Track};
use notwig_core::*;
use notwig_project::{
    BundleFiles, decode_audio_bytes, open_files, save_files, write_wav_f32_bytes,
};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Browser Project".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

#[test]
fn bundle_files_roundtrip_through_bytes() {
    let mut doc = doc();
    doc.transport.playing = true;
    doc.transport.recording = true;

    let files = save_files(&doc, BundleFiles::default()).unwrap();
    assert!(files.get("project.json").is_some());

    let loaded = open_files(&files).unwrap();
    assert_eq!(loaded.doc.meta.name, doc.meta.name);
    assert!(!loaded.doc.transport.playing);
    assert!(!loaded.doc.transport.recording);
    assert_eq!(loaded.doc.tracks.len(), doc.tracks.len());
    assert_eq!(loaded.issues.len(), 0);
}

#[test]
fn open_files_reports_missing_assets() {
    let mut doc = doc();
    let asset_id: AssetId = id::<Asset>(7);
    doc.assets.assets.insert(
        asset_id,
        AssetEntry {
            id: asset_id,
            kind: AssetKind::Audio,
            uri: "assets/missing.wav".into(),
            original_name: Some("missing.wav".into()),
            hash: None,
            format: "wav".into(),
            sample_rate: Some(48_000),
            channels: Some(2),
            duration_samples: Some(1),
            original_sample_rate: Some(48_000),
            original_channels: Some(2),
        },
    );
    let files = save_files(&doc, BundleFiles::default()).unwrap();

    let loaded = open_files(&files).unwrap();
    assert!(
        loaded
            .issues
            .iter()
            .any(|issue| issue.message.contains("missing.wav"))
    );
}

#[test]
fn open_files_without_manifest_is_an_error() {
    assert!(open_files(&BundleFiles::default()).is_err());
}

#[test]
fn wav_bytes_roundtrip() {
    let samples: Vec<f32> = (0..480)
        .map(|frame| (frame as f32 * 0.05).sin() * 0.25)
        .collect();
    let bytes = write_wav_f32_bytes(48_000, 1, &samples).unwrap();
    let decoded = decode_audio_bytes("tone.wav", bytes).unwrap();
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 1);
    assert_eq!(decoded.samples.len(), samples.len());
    for (decoded, original) in decoded.samples.iter().zip(&samples) {
        assert!((decoded - original).abs() < 1.0e-6);
    }
}
