use notwig_core::SchemaVersion;
use serde::{Deserialize, Serialize};

use crate::abi::ABI_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionCapability {
    #[serde(rename = "device")]
    Device,
    #[serde(rename = "ui.draw")]
    UiDraw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionDeviceDecl {
    pub type_id: String,
    #[serde(default = "default_schema_version")]
    pub version: SchemaVersion,
    #[serde(default)]
    pub name: Option<String>,
}

const fn default_schema_version() -> SchemaVersion {
    SchemaVersion::V0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default = "default_entry", alias = "entrypoint")]
    pub entry: String,
    #[serde(default)]
    pub capabilities: Vec<ExtensionCapability>,
    #[serde(default)]
    pub devices: Vec<ExtensionDeviceDecl>,
}

fn default_entry() -> String {
    "extension.wasm".into()
}

impl ExtensionManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension id must not be empty".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension name must not be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension version must not be empty".into(),
            ));
        }
        if self.entry.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension entry must not be empty".into(),
            ));
        }
        if self.api_version != ABI_VERSION {
            return Err(ManifestError::UnsupportedApi {
                found: self.api_version,
                expected: ABI_VERSION,
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        for device in &self.devices {
            if device.type_id.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "device type id must not be empty".into(),
                ));
            }
            if !seen.insert(&device.type_id) {
                return Err(ManifestError::Invalid(format!(
                    "duplicate device type id: {}",
                    device.type_id
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn has_capability(&self, capability: ExtensionCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ManifestError {
    #[error("invalid extension manifest: {0}")]
    Invalid(String),
    #[error("unsupported extension API version {found} (host supports {expected})")]
    UnsupportedApi { found: u32, expected: u32 },
    #[error("could not parse extension manifest: {0}")]
    Parse(String),
}
