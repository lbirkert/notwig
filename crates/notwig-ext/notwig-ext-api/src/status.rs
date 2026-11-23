use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::ExtensionCapability;

/// Lifecycle state of an installed extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionStatus {
    /// Found on disk but not yet loaded.
    Discovered,
    /// Loaded and ready to provide devices.
    Loaded,
    /// Installed but explicitly disabled in settings.
    Disabled,
    /// Discovery, validation, or loading failed; see `ExtensionInfo::error`.
    Failed,
}

impl ExtensionStatus {
    /// Returns the human-readable status label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Discovered => "Discovered",
            Self::Loaded => "Loaded",
            Self::Disabled => "Disabled",
            Self::Failed => "Failed",
        }
    }
}

/// Host-facing summary of an installed or discovered extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInfo {
    /// Stable extension id from the manifest.
    pub id: String,
    /// Display name from the manifest.
    pub name: String,
    /// Manifest vendor string.
    pub vendor: String,
    /// Manifest version string.
    pub version: String,
    /// Optional manifest description.
    pub description: Option<String>,
    /// Absolute path to the installed image, when present and found.
    pub image_path: Option<PathBuf>,
    /// Capabilities declared by the manifest.
    pub capabilities: Vec<ExtensionCapability>,
    /// Current lifecycle state.
    pub status: ExtensionStatus,
    /// Whether the extension is enabled in settings.
    pub enabled: bool,
    /// Number of device types known from loaded specs or manifest declarations.
    pub device_count: usize,
    /// Entrypoint targets advertised by the installed or indexed manifest.
    #[serde(default)]
    pub targets: Vec<String>,
    /// SHA-256 of the ZIP that installed this extension, when known.
    #[serde(default)]
    pub installed_hash: Option<String>,
    /// Failure detail when `status` is [`ExtensionStatus::Failed`].
    pub error: Option<String>,
}

/// Persisted per-extension settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSettings {
    /// Whether the extension should be loaded.
    pub enabled: bool,
    /// SHA-256 of the ZIP last installed through Notwig.
    #[serde(default)]
    pub installed_hash: Option<String>,
}

impl Default for ExtensionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            installed_hash: None,
        }
    }
}
