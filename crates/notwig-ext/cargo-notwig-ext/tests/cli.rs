use std::collections::BTreeMap;
use std::fs;
use std::io::Read;

use cargo_notwig_ext::build::{PackageEntry, package_entries, package_extension, package_targets};
use cargo_notwig_ext::scaffold;
use cargo_notwig_ext::target;
use notwig_ext_api::ExtensionManifest;
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
          "api_version": 8,
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
    scaffold::new(path.clone(), None, Some("com.test.my-gain".into())).unwrap();

    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("src/lib.rs").is_file());
    assert!(path.join("icon.png").is_file());
    let library = fs::read_to_string(path.join("src/lib.rs")).unwrap();
    assert!(library.contains("#[notwig_ext(load)]"));
    assert!(library.contains("com.test.my-gain"));

    let manifest: Value =
        serde_json::from_slice(&fs::read(path.join("extension.json")).unwrap()).unwrap();
    assert_eq!(manifest["id"], "com.test.my-gain");
    assert!(manifest.get("entrypoints").is_none());
    assert!(manifest.get("includes").is_none());
}

#[test]
fn binary_scaffolds_directly_and_through_cargo() {
    let temp = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_cargo-notwig-ext");
    for (directory, cargo_prefix) in [("direct-gain", None), ("cargo-gain", Some("notwig-ext"))] {
        let path = temp.path().join(directory);
        let mut command = std::process::Command::new(binary);
        if let Some(prefix) = cargo_prefix {
            command.arg(prefix);
        }
        let status = command
            .arg("new")
            .arg(&path)
            .arg("--id")
            .arg(format!("com.test.{directory}"))
            .status()
            .unwrap();
        assert!(status.success());
        assert!(path.join("Cargo.toml").is_file());
        assert!(path.join("extension.json").is_file());
    }
}

#[test]
fn default_target_plan_covers_every_supported_platform() {
    assert_eq!(target::all().len(), 7);
    let wasm = target::resolve("wasm").unwrap();
    assert_eq!(wasm.extension, "wasm");
    let windows = target::resolve("windows-x86_64").unwrap();
    assert_eq!(windows.extension, "dll");
    assert!(target::resolve("macos-arm64").is_some());
}

#[test]
fn packages_multiple_entrypoints_together() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path();
    write(&source.join("icon.png"), b"icon");
    write(&source.join("shared.txt"), b"shared");
    write(&source.join("wasm.txt"), b"wasm");
    write(&source.join("mac.txt"), b"mac");
    let out = source.join("dist/bundle.zip");
    let manifest = source_manifest();
    let entries = [
        PackageEntry {
            target: target::resolve("wasm").unwrap(),
            entrypoint: "test_extension.wasm".into(),
            bytes: b"wasm-artifact".to_vec(),
        },
        PackageEntry {
            target: target::resolve("macos-aarch64").unwrap(),
            entrypoint: "test_extension.dylib".into(),
            bytes: b"mac-artifact".to_vec(),
        },
    ];

    package_entries(&manifest, source, &entries, &out).unwrap();

    let files = archive_entries(&out);
    assert_eq!(
        files.get("test_extension.wasm").map(Vec::as_slice),
        Some(b"wasm-artifact".as_slice())
    );
    assert_eq!(
        files.get("test_extension.dylib").map(Vec::as_slice),
        Some(b"mac-artifact".as_slice())
    );
    assert!(files.contains_key("shared.txt"));
    assert!(files.contains_key("wasm.txt"));
    assert!(files.contains_key("mac.txt"));

    let packaged: Value = serde_json::from_slice(&files["extension.json"]).unwrap();
    assert_eq!(packaged["entrypoints"].as_array().unwrap().len(), 2);
    assert!(packaged.get("includes").is_none());
}

#[test]
fn packages_artifacts_already_present_in_target() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path();
    write(
        &source.join("Cargo.toml"),
        br#"[package]
name = "test-extension"
version = "0.1.0"
edition = "2024"

[lib]
name = "test_extension"
crate-type = ["cdylib", "rlib"]

[workspace]
"#,
    );
    write(&source.join("src/lib.rs"), b"");
    write(&source.join("icon.png"), b"icon");
    write(&source.join("shared.txt"), b"shared");
    write(&source.join("wasm.txt"), b"wasm");
    write(&source.join("mac.txt"), b"mac");
    write(
        &source.join("extension.json"),
        serde_json::to_vec(&source_manifest()).unwrap().as_slice(),
    );
    write(
        &source.join("target/wasm32-unknown-unknown/release/test_extension.wasm"),
        b"wasm-artifact",
    );
    write(
        &source.join("target/aarch64-apple-darwin/release/libtest_extension.dylib"),
        b"mac-artifact",
    );
    let out_dir = source.join("dist");

    let packaged = package_targets(&[], Some(source), &out_dir, None).unwrap();
    assert_eq!(packaged.file_name().unwrap(), "com.test.extension.zip");
    let files = archive_entries(&packaged);
    assert!(files.contains_key("test_extension.wasm"));
    assert!(files.contains_key("test_extension.dylib"));
    assert!(files.contains_key("icon.png"));
    assert!(files.contains_key("wasm.txt"));
    assert!(files.contains_key("mac.txt"));
    let manifest: Value = serde_json::from_slice(&files["extension.json"]).unwrap();
    assert_eq!(manifest["entrypoints"].as_array().unwrap().len(), 2);
}
