use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use notwig_ext::{read_archive, read_extension_files};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn manifest(id: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": id,
        "name": name,
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 8,
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

fn path(name: &str) -> PathBuf {
    PathBuf::from(name)
}

#[test]
fn reads_root_layout() {
    let zip = make_zip(&[
        (
            "extension.json",
            &manifest("com.test.archive", "Test Archive"),
        ),
        ("test.wasm", b"wasm"),
        ("icon.png", b"png"),
    ]);
    let archive = read_archive(&zip, Some("com.test.archive")).unwrap();
    assert_eq!(archive.manifest.name, "Test Archive");
    assert_eq!(
        archive.files.get(&path("test.wasm")),
        Some(&b"wasm".to_vec())
    );
    assert_eq!(archive.files.get(&path("icon.png")), Some(&b"png".to_vec()));
}

#[test]
fn reads_single_directory_layout_and_drops_outside_files() {
    let zip = make_zip(&[
        (
            "package/extension.json",
            &manifest("com.test.archive", "Test Archive"),
        ),
        ("package/test.wasm", b"wasm"),
        ("README.md", b"ignored"),
    ]);
    let archive = read_archive(&zip, None).unwrap();
    assert!(archive.files.contains_key(&path("test.wasm")));
    assert!(!archive.files.contains_key(&path("README.md")));
}

#[test]
fn rejects_unsafe_archive_paths() {
    let zip = make_zip(&[
        (
            "extension.json",
            &manifest("com.test.archive", "Test Archive"),
        ),
        ("../escape.txt", b"nope"),
    ]);
    assert!(read_archive(&zip, None).is_err());
}

#[test]
fn reads_installed_directory_files() {
    let mut files = BTreeMap::new();
    files.insert(
        path("extension.json"),
        manifest("com.test.directory", "Test Directory"),
    );
    files.insert(path("modules/test.wasm"), b"wasm".to_vec());
    let archive = read_extension_files(files, Some("com.test.directory")).unwrap();
    assert_eq!(archive.manifest.id, "com.test.directory");
    assert_eq!(
        archive.files.get(&path("modules/test.wasm")),
        Some(&b"wasm".to_vec())
    );
}

#[test]
fn rejects_expected_id_mismatch() {
    let zip = make_zip(&[
        (
            "extension.json",
            &manifest("com.test.archive", "Test Archive"),
        ),
        ("test.wasm", b"wasm"),
    ]);
    assert!(read_archive(&zip, Some("com.test.other")).is_err());
}
