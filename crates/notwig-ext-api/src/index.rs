use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::abi::ABI_VERSION;
use crate::manifest::{ExtensionCapability, ManifestError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionIndexDevice {
    pub name: String,
    pub type_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionIndexEntry {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub api_version: u32,
    pub entrypoint: String,
    #[serde(default)]
    pub capabilities: Vec<ExtensionCapability>,
    #[serde(default)]
    pub devices: Vec<ExtensionIndexDevice>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    pub url: String,
}

impl ExtensionIndexEntry {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension index id must not be empty".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension index name must not be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension index version must not be empty".into(),
            ));
        }
        if self.entrypoint.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension index entrypoint must not be empty".into(),
            ));
        }
        if self.url.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "extension index url must not be empty".into(),
            ));
        }
        if let Some(image) = &self.image {
            let path = Path::new(image);
            if image.trim().is_empty()
                || path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(ManifestError::Invalid(
                    "extension index image must be a relative path".into(),
                ));
            }
        }
        if self.api_version != ABI_VERSION {
            return Err(ManifestError::UnsupportedApi {
                found: self.api_version,
                expected: ABI_VERSION,
            });
        }
        let mut seen = BTreeSet::new();
        for device in &self.devices {
            if device.type_id.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "extension index device type id must not be empty".into(),
                ));
            }
            if !seen.insert(&device.type_id) {
                return Err(ManifestError::Invalid(format!(
                    "duplicate extension index device type id: {}",
                    device.type_id
                )));
            }
        }
        Ok(())
    }
}
