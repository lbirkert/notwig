use std::io::{Cursor, Write};
use std::path::PathBuf;

use notwig_ext::{install_zip_bytes, uninstall};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn manifest(id: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": id,
        "name": name,
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 2,
        "entrypoints": [{ "entrypoint": "test.wasm", "target": "wasm" }]
    }))
    .unwrap()
}

fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("notwig-install-{}", uuid::Uuid::new_v4()))
}

#[test]
fn installs_root_layout() {
    let root = temp_root();
    let zip = make_zip(&[
        (
            "extension.json",
            &manifest("com.test.install", "Test Install"),
        ),
        ("test.wasm", b"wasm"),
    ]);
    let installed = install_zip_bytes(&root, &zip, Some("com.test.install")).unwrap();
    assert_eq!(installed.manifest.name, "Test Install");
    assert!(installed.path.join("extension.json").exists());
    assert!(installed.path.join("test.wasm").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installs_single_directory_layout() {
    let root = temp_root();
    let zip = make_zip(&[
        (
            "package/extension.json",
            &manifest("com.test.install", "Test Install"),
        ),
        ("package/test.wasm", b"wasm"),
        ("README.md", b"ignored"),
    ]);
    let installed = install_zip_bytes(&root, &zip, None).unwrap();
    assert!(installed.path.join("extension.json").exists());
    assert!(!installed.path.join("README.md").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_unsafe_archive_paths() {
    let root = temp_root();
    let zip = make_zip(&[
        (
            "extension.json",
            &manifest("com.test.install", "Test Install"),
        ),
        ("../escape.txt", b"nope"),
    ]);
    assert!(install_zip_bytes(&root, &zip, None).is_err());
    assert!(!root.join("escape.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_expected_id_mismatch() {
    let root = temp_root();
    let zip = make_zip(&[("extension.json", &manifest("com.test.other", "Other"))]);
    assert!(install_zip_bytes(&root, &zip, Some("com.test.install")).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn replaces_existing_install_and_uninstalls() {
    let root = temp_root();
    let first = make_zip(&[
        ("extension.json", &manifest("com.test.install", "First")),
        ("first.wasm", b"one"),
    ]);
    install_zip_bytes(&root, &first, None).unwrap();
    let second = make_zip(&[
        ("extension.json", &manifest("com.test.install", "Second")),
        ("second.wasm", b"two"),
    ]);
    let installed = install_zip_bytes(&root, &second, None).unwrap();
    assert_eq!(installed.manifest.name, "Second");
    assert!(installed.path.join("second.wasm").exists());
    assert!(!installed.path.join("first.wasm").exists());
    uninstall(&root, "com.test.install").unwrap();
    assert!(!root.join("com.test.install").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_missing_manifest() {
    let root = temp_root();
    let zip = make_zip(&[("test.wasm", b"wasm")]);
    assert!(install_zip_bytes(&root, &zip, None).is_err());
    let _ = std::fs::remove_dir_all(root);
}
