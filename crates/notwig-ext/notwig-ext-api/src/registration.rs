//! Runtime registration descriptors exported after the load phase.

use serde::{Deserialize, Serialize};

use crate::audio_editor::AudioSubEditorDescriptor;
use crate::command::CommandDescriptor;

/// Current runtime registration manifest version.
pub const REGISTRATION_MANIFEST_VERSION: u32 = 1;

/// One extension point declared during preload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPointDescriptor {
    /// Stable namespaced point id.
    pub id: String,
}

/// One device implementation registered on the `notwig.devices` point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRegistrationDescriptor {
    /// Point the device implementation belongs to.
    pub point: String,
    /// Stable device type id.
    pub type_id: String,
}

/// Bounded registration manifest returned by a loaded extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionRegistrations {
    /// Manifest schema version.
    #[serde(default = "manifest_version")]
    pub version: u32,
    /// Points declared during preload.
    #[serde(default)]
    pub points: Vec<ExtensionPointDescriptor>,
    /// Device implementations registered during load.
    #[serde(default)]
    pub devices: Vec<DeviceRegistrationDescriptor>,
    /// Command implementations registered during load.
    #[serde(default)]
    pub commands: Vec<CommandDescriptor>,
    /// Audio sub-editor implementations registered during load.
    #[serde(default)]
    pub audio_sub_editors: Vec<AudioSubEditorDescriptor>,
}

const fn manifest_version() -> u32 {
    REGISTRATION_MANIFEST_VERSION
}

impl Default for ExtensionRegistrations {
    fn default() -> Self {
        Self {
            version: REGISTRATION_MANIFEST_VERSION,
            points: Vec::new(),
            devices: Vec::new(),
            commands: Vec::new(),
            audio_sub_editors: Vec::new(),
        }
    }
}
