use std::path::PathBuf;

use notwig_core::ids::{Asset, DeviceChain, Track};
use notwig_core::*;
use notwig_project::{
    autosave, cleanup_autosaves, create_session_lock, has_session_lock, import_asset,
    list_autosaves, load_latest_autosave, open, remove_session_lock, save, write_wav_f32,
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
fn autosave_writes_atomically_and_leaves_no_temporary_file() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    let doc = doc();
    save(&doc, &bundle).unwrap();

    let first = autosave(&doc, &bundle, 7).unwrap();
    assert!(first.exists());
    autosave(&doc, &bundle, 7).unwrap();

    let entries: Vec<PathBuf> = std::fs::read_dir(bundle.join("autosave"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(entries.len(), 1, "temporary snapshot was left behind");
    assert_eq!(entries[0].extension().unwrap(), "json");
    let recovered = load_latest_autosave(&bundle).unwrap().unwrap();
    assert_eq!(recovered.meta.name, doc.meta.name);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn wrapped_manifest_rejects_mismatched_payload_schema() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    std::fs::create_dir_all(&bundle).unwrap();
    let doc = doc();
    let mut manifest = notwig_project::Manifest::new(&doc);
    manifest.doc.schema = SchemaVersion::new(manifest.schema.major, manifest.schema.minor + 1);
    std::fs::write(
        bundle.join("project.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    assert!(matches!(
        open(&bundle),
        Err(notwig_project::ProjectError::Schema(_))
    ));
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

#[test]
fn decodes_mp3_at_its_own_sample_rate() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.mp3");
    let decoded = notwig_project::decode_audio(&path).unwrap();
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.channels, 1);
    assert!(decoded.frame_count() > 1000);
}

#[test]
fn imports_mp3_and_resamples_to_project_rate() {
    let dir = temp_dir();
    let bundle = dir.join("Mp3.notwig");
    std::fs::create_dir_all(&bundle).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.mp3");
    let imported = import_asset(&source, &bundle, id::<Asset>(501), 48_000).unwrap();
    assert_eq!(imported.entry.sample_rate, Some(48_000));
    assert_eq!(imported.entry.original_sample_rate, Some(44_100));
    assert_eq!(imported.entry.channels, Some(2));
    assert!(imported.buffer.frame_count() > 1000);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn decodes_integer_and_float_wav() {
    let dir = temp_dir();
    let int_path = dir.join("int.wav");
    let float_path = dir.join("float.wav");
    let int_spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut int_writer = hound::WavWriter::create(&int_path, int_spec).unwrap();
    for sample in [0i16, 1000, -1000, 2000] {
        int_writer.write_sample(sample).unwrap();
    }
    int_writer.finalize().unwrap();

    let float_spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut float_writer = hound::WavWriter::create(&float_path, float_spec).unwrap();
    for sample in [0.0_f32, 0.25, -0.25, 0.5, -0.5, 0.75, -0.75, 1.0] {
        float_writer.write_sample(sample).unwrap();
    }
    float_writer.finalize().unwrap();

    let int_decoded = notwig_project::decode_audio(&int_path).unwrap();
    assert_eq!(int_decoded.sample_rate, 44_100);
    assert_eq!(int_decoded.channels, 1);
    assert_eq!(int_decoded.frame_count(), 4);

    let float_decoded = notwig_project::decode_audio(&float_path).unwrap();
    assert_eq!(float_decoded.sample_rate, 48_000);
    assert_eq!(float_decoded.channels, 2);
    assert_eq!(float_decoded.frame_count(), 4);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn session_lock_lifecycle_controls_recovery_offer() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    let doc = doc();
    save(&doc, &bundle).unwrap();

    assert!(!open(&bundle).unwrap().recovery_offered);
    assert!(!has_session_lock(&bundle));

    create_session_lock(&bundle).unwrap();
    assert!(has_session_lock(&bundle));
    assert!(!open(&bundle).unwrap().recovery_offered);

    autosave(&doc, &bundle, 10).unwrap();
    assert!(open(&bundle).unwrap().recovery_offered);

    remove_session_lock(&bundle).unwrap();
    assert!(!has_session_lock(&bundle));
    assert!(!open(&bundle).unwrap().recovery_offered);

    remove_session_lock(&bundle).unwrap();
    cleanup_autosaves(&bundle).unwrap();
    assert_eq!(list_autosaves(&bundle).len(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_autosave_returns_error_instead_of_panicking() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    save(&doc(), &bundle).unwrap();
    autosave(&doc(), &bundle, 1).unwrap();
    std::fs::write(bundle.join("autosave/snapshot-0-2.json"), b"not json").unwrap();

    assert!(load_latest_autosave(&bundle).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
