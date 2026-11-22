use std::collections::BTreeMap;

use notwig_ext_api::config::{default_index_url_for_base, is_enabled};
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
fn config_enabled_defaults_to_true_and_honors_overrides() {
    let mut config = ExtensionConfig::default();
    assert!(config.is_enabled("com.test.unknown"));

    config.settings.insert(
        "com.test.enabled".into(),
        ExtensionSettings { enabled: true },
    );
    config.settings.insert(
        "com.test.disabled".into(),
        ExtensionSettings { enabled: false },
    );

    assert!(config.is_enabled("com.test.enabled"));
    assert!(!config.is_enabled("com.test.disabled"));
    assert!(is_enabled(&config.settings, "com.test.unknown"));
    assert!(!is_enabled(&config.settings, "com.test.disabled"));
}

#[test]
fn disabled_settings_filter_the_package_load_set() {
    let mut config = ExtensionConfig::default();
    config.settings.insert(
        "com.test.disabled".into(),
        ExtensionSettings { enabled: false },
    );

    let packages = ["com.test.enabled", "com.test.disabled"];
    let loaded = packages
        .into_iter()
        .filter(|id| config.is_enabled(id))
        .collect::<Vec<_>>();

    assert_eq!(loaded, vec!["com.test.enabled"]);
}

#[test]
fn wasm_entries_require_a_wasm_download() {
    let both_targets = first_entry();
    assert!(both_targets.is_available_to_wasm());
    assert_eq!(
        both_targets
            .download_for_wasm()
            .map(|download| download.url.as_str()),
        Some("extensions/ext-devices-wasm.zip")
    );

    let mut native_only = first_entry();
    native_only
        .urls
        .retain(|download| !download.targets.iter().any(|target| target == "wasm"));
    assert_eq!(native_only.urls.len(), 1);
    assert!(native_only.download_for_wasm().is_none());
    assert!(!native_only.is_available_to_wasm());
}

#[test]
fn default_index_resolves_against_a_site_base() {
    assert_eq!(
        default_index_url_for_base("https://example.com"),
        "https://example.com/extensions.json"
    );
    assert_eq!(
        default_index_url_for_base("https://example.com/notwig/"),
        "https://example.com/notwig/extensions.json"
    );
    assert_eq!(default_index_url_for_base(""), DEFAULT_INDEX_URL);
}

#[test]
fn resolving_default_index_leaves_custom_urls_alone() {
    let mut config = ExtensionConfig {
        version: CONFIG_VERSION,
        indexes: vec![
            DEFAULT_INDEX_URL.into(),
            "https://custom.example/extensions.json".into(),
        ],
        settings: BTreeMap::new(),
    };

    config.resolve_default_index("https://self.example/notwig");

    assert_eq!(
        config.indexes,
        vec![
            "https://self.example/notwig/extensions.json".to_string(),
            "https://custom.example/extensions.json".to_string(),
        ]
    );
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
