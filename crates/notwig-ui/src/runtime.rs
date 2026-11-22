use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::{AssetId, DeviceId, DeviceKind, TimeMus, TrackId};
use notwig_ext_api::{DeviceMeterSnapshot, ExtensionIndexEntry, ExtensionInfo};

use crate::clipboard::Clipboard;
use crate::waveform::{AudioWaveform, WaveformCache};

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeterValues {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub type_id: String,
    pub name: String,
    pub vendor: String,
    pub is_instrument: bool,
    pub kind: DeviceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioSettingsValue {
    pub driver: String,
    pub device: String,
    pub sample_rate: String,
    pub block_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioInputView {
    pub driver: String,
    pub device: String,
    pub label: String,
    pub channels: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioDeviceView {
    pub id: String,
    pub label: String,
    pub sample_rates: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioDriverView {
    pub id: String,
    pub label: String,
    pub devices: Vec<AudioDeviceView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioSettingsView {
    pub drivers: Vec<AudioDriverView>,
    pub current: AudioSettingsValue,
    pub error: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspAnalysisView {
    pub history: Vec<f32>,
    pub block_size: u32,
    pub sample_rate: u32,
    pub deadline_ms: f32,
    pub load_avg_ms: f32,
    pub load_max_ms: f32,
    pub period_jitter_percent: f32,
    pub input_latency_samples: u64,
    pub output_latency_samples: u64,
    pub timing: String,
}

impl Default for DspAnalysisView {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            block_size: 512,
            sample_rate: 48_000,
            deadline_ms: 512.0 / 48_000.0 * 1000.0,
            load_avg_ms: 0.0,
            load_max_ms: 0.0,
            period_jitter_percent: 0.0,
            input_latency_samples: 0,
            output_latency_samples: 0,
            timing: "Device".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProjectView {
    pub label: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectPickerAction {
    Open,
    SaveAs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPickerView {
    pub epoch: u64,
    pub action: ProjectPickerAction,
    pub projects: Vec<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub suggested_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeView {
    pub playing: bool,
    pub playhead: TimeMus,
    pub play_start: TimeMus,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub device_meters: BTreeMap<DeviceId, DeviceMeterSnapshot>,
    pub waveforms: Arc<WaveformCache>,
    pub audio_durations: BTreeMap<PathBuf, Option<f64>>,
    pub audio_analysis_pending: BTreeSet<PathBuf>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub status: Option<String>,
    pub engine_crashed: bool,
    pub engine_stopped: bool,
    pub audio_device_lost: bool,
    pub audio_device_lost_message: Option<String>,
    pub plugins: Vec<PluginEntry>,
    pub bundle_path: Option<PathBuf>,
    pub dirty: bool,
    pub saving: bool,
    pub audio_settings: AudioSettingsView,
    pub audio_inputs: Vec<AudioInputView>,
    pub dsp: DspAnalysisView,
    pub clipboard: Clipboard,
    pub extensions: Vec<ExtensionInfo>,
    pub extensions_error: Option<String>,
    pub extension_errors: Vec<String>,
    pub extension_icons: BTreeMap<String, Arc<Vec<u8>>>,
    pub extension_indexes: Vec<String>,
    pub extension_index: Vec<ExtensionIndexEntry>,
    pub extension_index_errors: Vec<String>,
    pub extension_index_loading: bool,
    pub extension_index_loaded: bool,
    pub recent_projects: Vec<RecentProjectView>,
    pub project_picker: Option<ProjectPickerView>,
}

impl Default for RuntimeView {
    fn default() -> Self {
        Self {
            playing: false,
            playhead: TimeMus::zero(),
            play_start: TimeMus::zero(),
            meters: BTreeMap::new(),
            device_meters: BTreeMap::new(),
            waveforms: Arc::default(),
            audio_durations: BTreeMap::new(),
            audio_analysis_pending: BTreeSet::new(),
            xruns: 0,
            cpu_load: None,
            status: None,
            engine_crashed: false,
            engine_stopped: false,
            audio_device_lost: false,
            audio_device_lost_message: None,
            plugins: Vec::new(),
            bundle_path: None,
            dirty: false,
            saving: false,
            audio_settings: AudioSettingsView::default(),
            audio_inputs: Vec::new(),
            dsp: DspAnalysisView::default(),
            clipboard: Clipboard::default(),
            extensions: Vec::new(),
            extensions_error: None,
            extension_errors: Vec::new(),
            extension_icons: BTreeMap::new(),
            extension_indexes: Vec::new(),
            extension_index: Vec::new(),
            extension_index_errors: Vec::new(),
            extension_index_loading: false,
            extension_index_loaded: false,
            recent_projects: Vec::new(),
            project_picker: None,
        }
    }
}

impl RuntimeView {
    pub fn insert_waveform(&mut self, asset_id: AssetId, waveform: AudioWaveform) {
        Arc::make_mut(&mut self.waveforms).insert(asset_id, waveform);
    }
}
