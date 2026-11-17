use std::path::PathBuf;

use notwig_core::ids::{Asset, DeviceChain, Track};
use notwig_core::*;
use notwig_project::{
    autosave, import_asset, list_autosaves, load_latest_autosave, open, save, write_wav_f32,
};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Project".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap()
}

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("notwig-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn save_and_open_roundtrip_normalizes_transport() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    let mut doc = doc();
    doc.transport.playing = true;
    save(&doc, &bundle).unwrap();
    let loaded = open(&bundle).unwrap();
    assert!(!loaded.doc.transport.playing);
    assert_eq!(loaded.doc.tracks.len(), doc.tracks.len());
    assert_eq!(loaded.issues.len(), 0);
    assert!(bundle.join("project.json").exists());
    assert!(bundle.join("assets").is_dir());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn autosave_snapshots_are_listed_and_recoverable() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    let doc = doc();
    save(&doc, &bundle).unwrap();
    autosave(&doc, &bundle, 1).unwrap();
    autosave(&doc, &bundle, 2).unwrap();
    assert_eq!(list_autosaves(&bundle).len(), 2);
    let recovered = load_latest_autosave(&bundle).unwrap().unwrap();
    assert_eq!(recovered.meta.name, doc.meta.name);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_resamples_and_duplicates_mono() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    std::fs::create_dir_all(&bundle).unwrap();
    let source = dir.join("source.wav");
    let samples: Vec<f32> = (0..1024).map(|i| (i as f32 / 32.0).sin() * 0.5).collect();
    write_wav_f32(&source, 44_100, 1, &samples).unwrap();
    let asset_id = id::<Asset>(500);
    let imported = import_asset(&source, &bundle, asset_id, 48_000).unwrap();
    assert_eq!(imported.entry.sample_rate, Some(48_000));
    assert_eq!(imported.entry.channels, Some(2));
    assert_eq!(imported.entry.original_sample_rate, Some(44_100));
    assert_eq!(imported.buffer.channels, 2);
    assert!(imported.buffer.frame_count() > 1024);
    let stored = bundle.join(&imported.entry.uri);
    assert!(stored.exists());
    let decoded = notwig_project::decode_audio(&stored).unwrap();
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aiff_decoder_reads_pcm() {
    let dir = temp_dir();
    let path = dir.join("test.aiff");
    let channels = 2u16;
    let frames = 4u32;
    let mut sound = Vec::new();
    for frame in 0..frames {
        for channel in 0..channels {
            let value = i16::try_from(i32::try_from(frame).unwrap() * 2 + i32::from(channel))
                .unwrap()
                * 1000;
            sound.extend_from_slice(&value.to_be_bytes());
        }
    }
    let mut comm = Vec::new();
    comm.extend_from_slice(&channels.to_be_bytes());
    comm.extend_from_slice(&frames.to_be_bytes());
    comm.extend_from_slice(&16u16.to_be_bytes());
    comm.extend_from_slice(&extended_80(44_100.0));
    let mut ssnd = Vec::new();
    ssnd.extend_from_slice(&0u32.to_be_bytes());
    ssnd.extend_from_slice(&0u32.to_be_bytes());
    ssnd.extend_from_slice(&sound);
    let mut body = Vec::new();
    body.extend_from_slice(b"AIFF");
    body.extend_from_slice(b"COMM");
    body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
    body.extend_from_slice(&comm);
    body.extend_from_slice(b"SSND");
    body.extend_from_slice(&(ssnd.len() as u32).to_be_bytes());
    body.extend_from_slice(&ssnd);
    let mut file = Vec::new();
    file.extend_from_slice(b"FORM");
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);
    std::fs::write(&path, file).unwrap();
    let decoded = notwig_project::decode_audio(&path).unwrap();
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.channels, 2);
    assert_eq!(decoded.frame_count(), 4);
    std::fs::remove_dir_all(dir).unwrap();
}

fn extended_80(value: f64) -> [u8; 10] {
    let mut bytes = [0u8; 10];
    let exponent = if value == 0.0 {
        0
    } else {
        value.abs().log2().floor() as i32 + 16383
    };
    bytes[0] = ((exponent >> 8) as u8) & 0x7f;
    bytes[1] = exponent as u8;
    let mantissa = if value == 0.0 {
        0
    } else {
        (value.abs() / 2.0_f64.powi(exponent - 16383) * 2.0_f64.powi(63)) as u64
    };
    bytes[2..10].copy_from_slice(&mantissa.to_be_bytes());
    bytes
}
