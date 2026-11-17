use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSettingsSnapshot {
    pub backend: String,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub available_devices: Vec<String>,
}

impl Default for AudioSettingsSnapshot {
    fn default() -> Self {
        Self {
            backend: "system".into(),
            output_device: None,
            sample_rate: 48_000,
            buffer_frames: 256,
            available_devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsSnapshot {
    pub audio: AudioSettingsSnapshot,
    pub plugin_scan_paths: Vec<PathBuf>,
    pub theme: String,
    pub ui_scale: f32,
    pub autosave_interval_secs: u64,
    pub recent_projects: Vec<PathBuf>,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            audio: AudioSettingsSnapshot::default(),
            plugin_scan_paths: Vec::new(),
            theme: "dark".into(),
            ui_scale: 1.0,
            autosave_interval_secs: 60,
            recent_projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsIntent {
    SetAudioDevice {
        backend: String,
        device: Option<String>,
    },
    SetBufferFrames(u32),
    SetSampleRate(u32),
    SetPluginScanPaths(Vec<PathBuf>),
    SetTheme(String),
    SetUiScale(f32),
    OpenRecent(PathBuf),
    ClearRecent,
}
