use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::status::ExtensionSettings;

pub const DEFAULT_INDEX_URL: &str = "https://notwig.lbirkert.com/extensions.json";
pub const CONFIG_VERSION: u32 = 2;

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
}
