use std::path::PathBuf;

use notwig_dsp::DeviceProcessor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    Vst3,
    Au,
    Vst2,
}

impl PluginFormat {
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Vst3 => "vst3",
            Self::Au => "au",
            Self::Vst2 => "vst2",
        }
    }

    #[must_use]
    pub fn enabled(self) -> bool {
        match self {
            Self::Vst3 => {
                cfg!(feature = "vst3")
                    || !cfg!(any(feature = "vst3", feature = "au", feature = "vst2"))
            }
            Self::Au => {
                cfg!(feature = "au")
                    || !cfg!(any(feature = "vst3", feature = "au", feature = "vst2"))
            }
            Self::Vst2 => {
                cfg!(feature = "vst2")
                    || !cfg!(any(feature = "vst3", feature = "au", feature = "vst2"))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub type_id: String,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub uid: String,
    pub is_instrument: bool,
    pub binary_size: u64,
    pub modified_utc: i64,
}

impl PluginDescriptor {
    #[must_use]
    pub fn type_id_for(format: PluginFormat, uid: &str) -> String {
        format!("{}.{}", format.prefix(), uid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub audio_inputs: u16,
    pub audio_outputs: u16,
    pub accepts_notes: bool,
    pub produces_notes: bool,
    pub has_editor: bool,
    pub supports_state: bool,
    /// Whether the host adapter can query latency for this instance.
    pub reports_latency: bool,
    /// Reported processing latency in samples; only meaningful when
    /// `reports_latency` is true.
    #[serde(default)]
    pub latency_samples: u32,
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self {
            audio_inputs: 2,
            audio_outputs: 2,
            accepts_notes: false,
            produces_notes: false,
            has_editor: false,
            supports_state: true,
            reports_latency: false,
            latency_samples: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginState {
    pub format: PluginFormat,
    pub plugin_uid: String,
    pub plugin_version: String,
    pub blob: Vec<u8>,
}

pub trait PluginInstance: DeviceProcessor {
    fn capabilities(&self) -> PluginCapabilities;
    fn save_state(&mut self) -> Result<PluginState, PluginError>;
    fn load_state(&mut self, state: &PluginState) -> Result<(), PluginError>;
    fn describe_parameters(&self) -> Vec<notwig_dsp::ParamSpec> {
        Vec::new()
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error("plugin format {0:?} is not enabled")]
    FormatDisabled(PluginFormat),
    #[error("plugin backend is not available for {0:?}")]
    BackendUnavailable(PluginFormat),
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin is blacklisted: {0}")]
    Blacklisted(String),
    #[error("failed to load plugin: {0}")]
    Load(String),
    #[error("plugin state is incompatible: {0}")]
    StateIncompatible(String),
    #[error("plugin scan failed: {0}")]
    Scan(String),
    #[error("io error: {0}")]
    Io(String),
}
