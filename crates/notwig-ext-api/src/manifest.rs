use std::path::{Component, Path};

use notwig_core::SchemaVersion;
use serde::{Deserialize, Serialize};

use crate::abi::ABI_VERSION;
use crate::target;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionCapability {
    #[serde(rename = "device")]
    Device,
    #[serde(rename = "ui.egui")]
    UiEgui,
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
pub struct ExtensionEntryPoint {
    #[serde(alias = "file")]
    pub entrypoint: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    pub version: String,
    pub api_version: u32,
    pub entrypoints: Vec<ExtensionEntryPoint>,
    #[serde(default)]
    pub capabilities: Vec<ExtensionCapability>,
    #[serde(default)]
    pub devices: Vec<ExtensionDeviceDecl>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
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
        if self.entrypoints.is_empty() {
            return Err(ManifestError::Invalid(
                "extension must declare at least one entrypoint".into(),
            ));
        }
        for entry in &self.entrypoints {
            if entry.entrypoint.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "extension entrypoint must not be empty".into(),
                ));
            }
            if entry.target.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "extension entrypoint target must not be empty".into(),
                ));
            }
            validate_relative_path(&entry.entrypoint, "extension entrypoint")?;
        }
        if self.api_version != ABI_VERSION {
            return Err(ManifestError::UnsupportedApi {
                found: self.api_version,
                expected: ABI_VERSION,
            });
        }
        if let Some(image) = &self.image {
            validate_relative_image(image)?;
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

    /// Returns the first entrypoint this host can load, in manifest order.
    #[must_use]
    pub fn entry_for(&self, supports: impl Fn(&str) -> bool) -> Option<&ExtensionEntryPoint> {
        self.entrypoints
            .iter()
            .find(|entry| supports(&entry.target))
    }

    /// Returns the first entrypoint matching a canonical target.
    #[must_use]
    pub fn entry_for_target(&self, wanted: &str) -> Option<&ExtensionEntryPoint> {
        let wanted = target::canonical(wanted)?;
        self.entrypoints
            .iter()
            .find(|entry| target::canonical(&entry.target) == Some(wanted))
    }
}

fn validate_relative_image(image: &str) -> Result<(), ManifestError> {
    validate_relative_path(image, "extension image")
}

fn validate_relative_path(value: &str, label: &str) -> Result<(), ManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManifestError::Invalid(format!(
            "{label} must be a relative path inside the package"
        )));
    }
    Ok(())
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
