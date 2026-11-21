use std::collections::BTreeMap;

use notwig_ext_api::{
    CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig, ExtensionIndexEntry, ExtensionManifest,
    ExtensionSettings,
};

const INDEX: &str = r#"[
  {
    "id": "notwig.ext.devices",
    "name": "Notwig Stock Devices",
    "vendor": "Notwig",
    "version": "0.1.0",
    "api_version": 2,
    "urls": [
      {
        "url": "extensions/ext-devices-macos-arm64.zip",
        "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "targets": ["macos-aarch64"]
      },
      {
        "url": "extensions/ext-devices-wasm.zip",
        "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "targets": ["wasm"]
      }
    ],
    "capabilities": ["device", "ui.draw"],
    "devices": [
      { "name": "Notwig Stock Limiter", "type_id": "notwig.ext.devices.limiter" }
    ],
    "description": "Stock devices.",
    "image": "icon.png"
  }
]"#;

fn first_entry() -> ExtensionIndexEntry {
    serde_json::from_str::<Vec<ExtensionIndexEntry>>(INDEX).unwrap()[0].clone()
}

#[test]
fn parses_and_validates_an_index() {
    let entries: Vec<ExtensionIndexEntry> = serde_json::from_str(INDEX).unwrap();
    assert_eq!(entries.len(), 1);
    entries[0].validate().unwrap();
    assert_eq!(entries[0].devices[0].type_id, "notwig.ext.devices.limiter");
    assert_eq!(entries[0].description.as_deref(), Some("Stock devices."));
    assert_eq!(entries[0].image.as_deref(), Some("icon.png"));
    assert_eq!(entries[0].urls[0].targets, vec!["macos-aarch64"]);
    assert_eq!(
        entries[0]
            .download_for(|target| target == "wasm")
            .map(|download| download.url.as_str()),
        Some("extensions/ext-devices-wasm.zip")
    );
    assert_eq!(
        entries[0]
            .download_for(|target| target == "macos-aarch64")
            .map(|download| download.url.as_str()),
        Some("extensions/ext-devices-macos-arm64.zip")
    );
}

#[test]
fn rejects_invalid_index_entries() {
    let mut entry = first_entry();
    entry.urls.clear();
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.urls[0].url.clear();
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.urls[0].hash = "not-a-hash".into();
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.urls[0].targets.clear();
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.api_version = 999;
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.devices.push(entry.devices[0].clone());
    assert!(entry.validate().is_err());

    let mut entry = first_entry();
    entry.image = Some("../escape.png".into());
    assert!(entry.validate().is_err());
}

#[test]
fn manifest_requires_entrypoints_and_accepts_aliases() {
    let manifest: ExtensionManifest = serde_json::from_str(
        r#"{"id":"com.test","name":"Test","version":"0.1.0","api_version":2,"entrypoints":[{"file":"native.dylib","target":"macos-arm64"},{"entrypoint":"plugin.wasm","target":"wasm"}]}"#,
    )
    .unwrap();
    manifest.validate().unwrap();
    assert_eq!(manifest.entrypoints[0].target, "macos-arm64");
    assert_eq!(manifest.entrypoints[0].entrypoint, "native.dylib");
    assert!(manifest.entry_for_target("macos-aarch64").is_some());
    assert!(manifest.entry_for_target("wasm").is_some());
}

#[test]
fn manifest_rejects_empty_entrypoints() {
    let manifest: ExtensionManifest = serde_json::from_str(
        r#"{"id":"com.test","name":"Test","version":"0.1.0","api_version":2,"entrypoints":[]}"#,
    )
    .unwrap();
    assert!(manifest.validate().is_err());
}

#[test]
fn manifest_rejects_path_traversal_images() {
    let manifest: ExtensionManifest = serde_json::from_str(
        r#"{"id":"com.test","name":"Test","version":"0.1.0","api_version":2,"entrypoints":[{"entrypoint":"extension.wasm","target":"wasm"}],"image":"../escape.png"}"#,
    )
    .unwrap();
    assert!(manifest.validate().is_err());
}

#[test]
fn manifest_rejects_legacy_entry_fields() {
    assert!(
        serde_json::from_str::<ExtensionManifest>(
            r#"{"id":"com.test","name":"Test","version":"0.1.0","api_version":2,"entry":"extension.wasm"}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<ExtensionManifest>(
            r#"{"id":"com.test","name":"Test","version":"0.1.0","api_version":2,"entrypoint":"extension.wasm"}"#,
        )
        .is_err()
    );
}

#[test]
fn config_round_trips_new_format() {
    let mut settings = BTreeMap::new();
    settings.insert(
        "notwig.ext.devices".into(),
        ExtensionSettings { enabled: false },
    );
    let config = ExtensionConfig {
        version: CONFIG_VERSION,
        indexes: vec!["https://example.com/extensions.json".into()],
        settings,
    };
    let text = config.to_json().unwrap();
    let parsed = ExtensionConfig::from_json(&text).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn default_index_points_at_the_official_host() {
    assert_eq!(
        DEFAULT_INDEX_URL,
        "https://notwig.lbirkert.com/extensions.json"
    );
    let config = ExtensionConfig::default();
    assert_eq!(config.indexes, vec![DEFAULT_INDEX_URL.to_string()]);
}

#[test]
fn legacy_settings_map_migrates_to_default_index() {
    let text = r#"{"notwig.ext.devices":{"enabled":true,"trust":"Trusted"}}"#;
    let config = ExtensionConfig::from_json(text).unwrap();
    assert_eq!(config.version, CONFIG_VERSION);
    assert_eq!(config.indexes, vec![DEFAULT_INDEX_URL.to_string()]);
    assert_eq!(
        config.settings.get("notwig.ext.devices"),
        Some(&ExtensionSettings { enabled: true })
    );
}

#[test]
fn invalid_config_is_rejected() {
    assert!(ExtensionConfig::from_json("not json").is_err());
}
