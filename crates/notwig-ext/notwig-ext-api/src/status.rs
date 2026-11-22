use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::ExtensionCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionStatus {
    Discovered,
    Loaded,
    Disabled,
    Failed,
}

impl ExtensionStatus {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: Option<String>,
    pub image_path: Option<PathBuf>,
    pub capabilities: Vec<ExtensionCapability>,
    pub status: ExtensionStatus,
    pub enabled: bool,
    pub device_count: usize,
    /// Entrypoint targets advertised by the installed or indexed manifest.
    #[serde(default)]
    pub targets: Vec<String>,
    /// SHA-256 of the ZIP that installed this extension, when known.
    #[serde(default)]
    pub installed_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSettings {
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
