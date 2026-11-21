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
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSettings {
    pub enabled: bool,
}

impl Default for ExtensionSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}
