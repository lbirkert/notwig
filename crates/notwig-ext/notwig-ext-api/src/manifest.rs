use std::path::{Component, Path};

use notwig_core::SchemaVersion;
use serde::{Deserialize, Serialize};

use crate::abi::ABI_VERSION;
use crate::target;

/// Optional feature an extension declares in its manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionCapability {
    /// Provides processor devices.
    #[serde(rename = "device")]
    Device,
    /// Provides custom egui panels for its devices.
    #[serde(rename = "ui.panel")]
    UiPanel,
}

/// One device type declared by an extension manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionDeviceDecl {
    /// Device type id that the extension registers at runtime.
    pub type_id: String,
    /// Schema version for the serialized device spec.
    #[serde(default = "default_schema_version")]
    pub version: SchemaVersion,
    /// Optional display name override.
    #[serde(default)]
    pub name: Option<String>,
}

const fn default_schema_version() -> SchemaVersion {
    SchemaVersion::V0
}

/// One target-specific entrypoint binary inside a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionEntryPoint {
    /// Relative path to the WASM module or native shared library.
    #[serde(alias = "file")]
    pub entrypoint: String,
    /// Target identifier such as `wasm` or `macos-aarch64`.
    pub target: String,
}

/// Extra file included in a package build.
///
/// Source manifests may list assets to bundle. An optional `target` limits the
/// include to one build target. `cargo notwig-ext` removes this list from the
/// packaged manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInclude {
    /// Relative path inside the extension source directory.
    pub path: String,
    /// Optional build target that gates this include.
    #[serde(default)]
    pub target: Option<String>,
}

/// Parsed `extension.json` manifest.
///
/// Unknown fields are rejected. Installed packages require at least one
/// entrypoint; source manifests validated by `cargo notwig-ext` may omit them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    /// Stable extension id; also used as the install directory name.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Vendor string, empty by default.
    #[serde(default)]
    pub vendor: String,
    /// Human-readable extension version.
    pub version: String,
    /// ABI version; must exactly equal [`crate::ABI_VERSION`].
    pub api_version: u32,
    /// Target-specific entrypoints, in host preference order.
    #[serde(default)]
    pub entrypoints: Vec<ExtensionEntryPoint>,
    /// Source-only asset includes consumed by `cargo notwig-ext`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<ExtensionInclude>,
    /// Declared optional capabilities.
    #[serde(default)]
    pub capabilities: Vec<ExtensionCapability>,
    /// Declared device types.
    #[serde(default)]
    pub devices: Vec<ExtensionDeviceDecl>,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional relative path to a package image.
    #[serde(default)]
    pub image: Option<String>,
}

impl ExtensionManifest {
    /// Validates an installed package manifest.
    ///
    /// Installed packages must declare at least one entrypoint.
    pub fn validate(&self) -> Result<(), ManifestError> {
        self.validate_with_entrypoints(true)
    }

    /// Validates a source manifest used by `cargo notwig-ext`.
    ///
    /// Source manifests omit `entrypoints`; the CLI adds the selected target
    /// entrypoint before packaging.
    pub fn validate_source(&self) -> Result<(), ManifestError> {
        self.validate_with_entrypoints(false)
    }

    fn validate_with_entrypoints(&self, require_entrypoints: bool) -> Result<(), ManifestError> {
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
        if require_entrypoints && self.entrypoints.is_empty() {
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
        for include in &self.includes {
            validate_relative_path(&include.path, "extension include")?;
            if let Some(target) = &include.target {
                if target::canonical(target).is_none() {
                    return Err(ManifestError::Invalid(format!(
                        "unknown include target: {target}"
                    )));
                }
            }
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

    /// Returns `true` when the manifest declares `capability`.
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

/// Validation failure for an extension manifest or index entry.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// A required field is empty or a path/identifier is malformed.
    #[error("invalid extension manifest: {0}")]
    Invalid(String),
    /// The manifest targets a different ABI version than this host.
    #[error("unsupported extension API version {found} (host supports {expected})")]
    UnsupportedApi { found: u32, expected: u32 },
    /// The manifest text or JSON could not be parsed.
    #[error("could not parse extension manifest: {0}")]
    Parse(String),
}
