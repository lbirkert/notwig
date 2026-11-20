use std::collections::BTreeMap;

use notwig_ext_api::{
    CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig, ExtensionIndexEntry, ExtensionSettings,
    TrustLevel,
};

const INDEX: &str = r#"[
  {
    "id": "notwig.ext.devices",
    "name": "Notwig Stock Devices",
    "vendor": "Notwig",
    "version": "0.1.0",
    "api_version": 1,
    "entrypoint": "notwig_ext_devices.wasm",
    "capabilities": ["device", "ui.draw"],
    "devices": [
      { "name": "Notwig Stock Limiter", "type_id": "notwig.ext.devices.limiter" }
    ],
    "url": "extensions/notwig.ext.devices.zip"
  }
]"#;

#[test]
fn parses_and_validates_an_index() {
    let entries: Vec<ExtensionIndexEntry> = serde_json::from_str(INDEX).unwrap();
    assert_eq!(entries.len(), 1);
    entries[0].validate().unwrap();
    assert_eq!(entries[0].devices[0].type_id, "notwig.ext.devices.limiter");
}

#[test]
fn rejects_invalid_index_entries() {
    let mut entry: ExtensionIndexEntry =
        serde_json::from_str::<Vec<ExtensionIndexEntry>>(INDEX).unwrap()[0].clone();
    entry.url.clear();
    assert!(entry.validate().is_err());

    entry.url = "extensions/test.zip".into();
    entry.api_version = 999;
    assert!(entry.validate().is_err());

    entry.api_version = 1;
    entry.devices.push(entry.devices[0].clone());
    assert!(entry.validate().is_err());
}

#[test]
fn config_round_trips_new_format() {
    let mut settings = BTreeMap::new();
    settings.insert(
        "notwig.ext.devices".into(),
        ExtensionSettings {
            enabled: false,
            trust: TrustLevel::Limited,
        },
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
fn legacy_settings_map_migrates_to_default_index() {
    let text = r#"{"notwig.ext.devices":{"enabled":true,"trust":"Trusted"}}"#;
    let config = ExtensionConfig::from_json(text).unwrap();
    assert_eq!(config.version, CONFIG_VERSION);
    assert_eq!(config.indexes, vec![DEFAULT_INDEX_URL.to_string()]);
    assert!(config.settings.contains_key("notwig.ext.devices"));
}

#[test]
fn invalid_config_is_rejected() {
    assert!(ExtensionConfig::from_json("not json").is_err());
}
