use std::path::{Path, PathBuf};

use notwig_core::ids::Asset;
use notwig_core::*;
use notwig_project::{decode_audio, import_asset, write_wav_f32};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("notwig-import-hash-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn source_tone(dir: &Path) -> PathBuf {
    let source = dir.join("source.wav");
    let samples: Vec<f32> = (0..1024).map(|i| (i as f32 / 32.0).sin() * 0.5).collect();
    write_wav_f32(&source, 44_100, 2, &samples).unwrap();
    source
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[test]
fn stored_filename_is_sha256_of_stored_bytes() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    std::fs::create_dir_all(&bundle).unwrap();
    let source = source_tone(&dir);

    let imported = import_asset(&source, &bundle, id::<Asset>(700), 48_000).unwrap();
    let stored = bundle.join(&imported.entry.uri);
    let bytes = std::fs::read(&stored).unwrap();
    let expected_hash = sha256_hex(&bytes);

    assert_eq!(
        stored.file_name().unwrap().to_str().unwrap(),
        format!("{expected_hash}.wav")
    );
    assert_eq!(imported.entry.hash.as_deref(), Some(expected_hash.as_str()));
    // The stored file round-trips through the decoder.
    let decoded = decode_audio(&stored).unwrap();
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn importing_same_source_twice_dedupes_and_leaves_no_temporary_file() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    std::fs::create_dir_all(&bundle).unwrap();
    let source = source_tone(&dir);

    let first = import_asset(&source, &bundle, id::<Asset>(701), 48_000).unwrap();
    let second = import_asset(&source, &bundle, id::<Asset>(702), 48_000).unwrap();

    assert_eq!(first.entry.uri, second.entry.uri);
    let temporary: Vec<PathBuf> = std::fs::read_dir(bundle.join("assets"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("tmp"))
        .collect();
    assert!(temporary.is_empty(), "temporary asset file was left behind");
    std::fs::remove_dir_all(dir).unwrap();
}
