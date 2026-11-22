use std::collections::BTreeMap;
use std::fs;
use std::io::Read;

use notwig_ext_api::ExtensionManifest;
use notwig_ext_cli::build::package_extension;
use notwig_ext_cli::scaffold;
use notwig_ext_cli::target;
use serde_json::Value;
use zip::ZipArchive;

fn write(path: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn source_manifest() -> ExtensionManifest {
    serde_json::from_str(
        r#"{
          "id": "com.test.extension",
          "name": "Test Extension",
          "vendor": "Test",
          "version": "0.1.0",
          "api_version": 4,
          "capabilities": ["device", "ui.panel"],
          "devices": [{"name": "Test", "type_id": "com.test.extension.device"}],
          "image": "icon.png",
          "includes": [
            {"path": "shared.txt"},
            {"path": "wasm.txt", "target": "wasm"},
            {"path": "mac.txt", "target": "macos-aarch64"}
          ]
        }"#,
    )
    .unwrap()
}

fn archive_entries(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    let file = fs::File::open(path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        entries.insert(entry.name().to_owned(), bytes);
    }
    entries
}

#[test]
fn packages_generated_manifest_and_filtered_includes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path();
    write(&source.join("icon.png"), b"icon");
    write(&source.join("shared.txt"), b"shared");
    write(&source.join("wasm.txt"), b"wasm");
    write(&source.join("mac.txt"), b"mac");
    let out = source.join("dist/test.zip");
    let target = target::resolve("wasm").unwrap();

    package_extension(
        &source_manifest(),
        source,
        target,
        "test_extension.wasm",
        b"artifact",
        &out,
    )
    .unwrap();

    let entries = archive_entries(&out);
    assert_eq!(
        entries.get("test_extension.wasm").map(Vec::as_slice),
        Some(b"artifact".as_slice())
    );
    assert_eq!(
        entries.get("icon.png").map(Vec::as_slice),
        Some(b"icon".as_slice())
    );
    assert_eq!(
        entries.get("shared.txt").map(Vec::as_slice),
        Some(b"shared".as_slice())
    );
    assert_eq!(
        entries.get("wasm.txt").map(Vec::as_slice),
        Some(b"wasm".as_slice())
    );
    assert!(!entries.contains_key("mac.txt"));

    let manifest: Value = serde_json::from_slice(&entries["extension.json"]).unwrap();
    assert_eq!(
        manifest["entrypoints"][0]["entrypoint"],
        "test_extension.wasm"
    );
    assert_eq!(manifest["entrypoints"][0]["target"], "wasm");
    assert!(manifest.get("includes").is_none());
    assert_eq!(manifest["image"], "icon.png");
}

#[test]
fn targets_gate_includes_independently() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path();
    write(&source.join("icon.png"), b"icon");
    write(&source.join("shared.txt"), b"shared");
    write(&source.join("wasm.txt"), b"wasm");
    write(&source.join("mac.txt"), b"mac");
    let out = source.join("dist/mac.zip");
    let target = target::resolve("macos-arm64").unwrap();

    package_extension(
        &source_manifest(),
        source,
        target,
        "test_extension.dylib",
        b"artifact",
        &out,
    )
    .unwrap();

    let entries = archive_entries(&out);
    assert!(entries.contains_key("mac.txt"));
    assert!(!entries.contains_key("wasm.txt"));
}

#[test]
fn new_scaffolds_a_buildable_extension() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("my-gain");
    scaffold::new_command(&[
        path.display().to_string(),
        "--id".into(),
        "com.test.my-gain".into(),
    ])
    .unwrap();

    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("src/lib.rs").is_file());
    assert!(path.join("icon.png").is_file());
    let library = fs::read_to_string(path.join("src/lib.rs")).unwrap();
    assert!(library.contains("#[notwig_extension]"));
    assert!(library.contains("com.test.my-gain"));

    let manifest: Value =
        serde_json::from_slice(&fs::read(path.join("extension.json")).unwrap()).unwrap();
    assert_eq!(manifest["id"], "com.test.my-gain");
    assert!(manifest.get("entrypoints").is_none());
    assert!(manifest.get("includes").is_none());
}
