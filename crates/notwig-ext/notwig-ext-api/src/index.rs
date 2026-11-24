use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::abi::ABI_VERSION;
use crate::manifest::{ExtensionCapability, ManifestError};
use crate::target;

/// A device advertised by an extension index entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionIndexDevice {
    /// Display name.
    pub name: String,
    /// Device type id.
    pub type_id: String,
}

/// One downloadable package in an extension index entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionIndexUrl {
    pub url: String,
    /// SHA-256 of the ZIP, as lowercase hex.
    pub hash: String,
    /// Compressed ZIP size in bytes; `0` means unknown.
    #[serde(default)]
    pub size: u64,
    /// Targets in the same order as the package's `extension.json`
    /// `entrypoints` list.
    pub targets: Vec<String>,
}

/// One installable extension listed in an extension index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionIndexEntry {
    /// Stable extension id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Vendor string.
    pub vendor: String,
    /// Human-readable version.
    pub version: String,
    /// ABI version; must exactly equal [`crate::ABI_VERSION`].
    pub api_version: u32,
    /// Downloadable packages for different target sets.
    #[serde(default)]
    pub urls: Vec<ExtensionIndexUrl>,
    /// Declared capabilities.
    #[serde(default)]
    pub capabilities: Vec<ExtensionCapability>,
    /// Advertised devices.
    #[serde(default)]
    pub devices: Vec<ExtensionIndexDevice>,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional relative image path.
    #[serde(default)]
    pub image: Option<String>,
}

impl ExtensionIndexEntry {
    /// Validates ids, versions, URLs, hashes, targets, image path, ABI version,
    /// and device uniqueness.
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
        if self.urls.is_empty() {
            return Err(ManifestError::Invalid(
                "extension index entry must list at least one url".into(),
            ));
        }
        let mut seen_urls = BTreeSet::new();
        for download in &self.urls {
            if download.url.trim().is_empty() {
                return Err(ManifestError::Invalid(
                    "extension index url must not be empty".into(),
                ));
            }
            if !seen_urls.insert(download.url.as_str()) {
                return Err(ManifestError::Invalid(format!(
                    "duplicate extension index url: {}",
                    download.url
                )));
            }
            if !valid_sha256(&download.hash) {
                return Err(ManifestError::Invalid(format!(
                    "extension index hash is not a SHA-256 hex digest: {}",
                    download.hash
                )));
            }
            if download.targets.is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "extension index url has no targets: {}",
                    download.url
                )));
            }
            for target_id in &download.targets {
                if target_id.trim().is_empty() {
                    return Err(ManifestError::Invalid(
                        "extension index target must not be empty".into(),
                    ));
                }
            }
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

    /// Returns the first download whose target list contains a supported
    /// target.
    #[must_use]
    pub fn download_for(&self, supports: impl Fn(&str) -> bool) -> Option<&ExtensionIndexUrl> {
        self.urls
            .iter()
            .find(|download| download.targets.iter().any(|target_id| supports(target_id)))
    }

    /// Returns the first download that supports this host.
    #[must_use]
    pub fn download_for_host(&self) -> Option<&ExtensionIndexUrl> {
        self.download_for(target::is_supported)
    }

    /// Returns the first download that contains a wasm target.
    #[must_use]
    pub fn download_for_wasm(&self) -> Option<&ExtensionIndexUrl> {
        self.download_for(target::is_wasm)
    }

    /// Returns `true` when the browser build can install this entry.
    #[must_use]
    pub fn is_available_to_wasm(&self) -> bool {
        self.download_for_wasm().is_some()
    }
}

fn valid_sha256(hash: &str) -> bool {
    let hash = hash.trim();
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}
