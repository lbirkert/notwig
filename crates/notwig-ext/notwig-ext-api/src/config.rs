use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::status::ExtensionSettings;

pub const DEFAULT_INDEX_URL: &str = "https://notwig.lbirkert.com/extensions.json";
pub const CONFIG_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub indexes: Vec<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, ExtensionSettings>,
}

const fn default_version() -> u32 {
    CONFIG_VERSION
}

impl Default for ExtensionConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            indexes: vec![DEFAULT_INDEX_URL.into()],
            settings: BTreeMap::new(),
        }
    }
}

impl ExtensionConfig {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if let Ok(config) = serde_json::from_str::<Self>(text) {
            if config.version <= CONFIG_VERSION {
                return Ok(config);
            }
            return Err(format!(
                "unsupported extension config version {}",
                config.version
            ));
        }
        if let Ok(settings) = serde_json::from_str::<BTreeMap<String, ExtensionSettings>>(text) {
            return Ok(Self {
                version: CONFIG_VERSION,
                indexes: vec![DEFAULT_INDEX_URL.into()],
                settings,
            });
        }
        Err("invalid extension config".into())
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|error| format!("encode extension config: {error}"))
    }

    /// Returns `false` only when the extension has an explicit disabled
    /// setting. Extensions without a stored setting are enabled by default.
    #[must_use]
    pub fn is_enabled(&self, id: &str) -> bool {
        is_enabled(&self.settings, id)
    }

    /// Replaces the built-in default index URL with one relative to `base`.
    /// Custom index URLs are left untouched, and an empty base keeps the
    /// official fallback.
    pub fn resolve_default_index(&mut self, base: &str) {
        let resolved = default_index_url_for_base(base);
        for index in &mut self.indexes {
            if index == DEFAULT_INDEX_URL {
                index.clone_from(&resolved);
            }
        }
    }
}

/// Returns `false` only when `settings` explicitly disables `id`.
#[must_use]
pub fn is_enabled(settings: &BTreeMap<String, ExtensionSettings>, id: &str) -> bool {
    settings.get(id).is_none_or(|settings| settings.enabled)
}

/// Builds the default index URL for a site base such as
/// `https://example.com` or `https://example.com/notwig`.
#[must_use]
pub fn default_index_url_for_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return DEFAULT_INDEX_URL.to_owned();
    }
    format!("{base}/extensions.json")
}
