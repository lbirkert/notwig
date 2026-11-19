use std::collections::BTreeMap;
use std::path::PathBuf;

use notwig_core::{AssetId, DeviceKind, TimeMus, TrackId};

use crate::clipboard::Clipboard;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeView {
    pub playing: bool,
    pub playhead: TimeMus,
    pub play_start: TimeMus,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub waveforms: BTreeMap<AssetId, Vec<f32>>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub status: Option<String>,
    pub plugins: Vec<PluginEntry>,
    pub bundle_path: Option<PathBuf>,
    pub dirty: bool,
    pub saving: bool,
    pub audio_settings: AudioSettingsView,
    pub dsp: DspAnalysisView,
    pub clipboard: Clipboard,
}

impl Default for RuntimeView {
    fn default() -> Self {
        Self {
            playing: false,
            playhead: TimeMus::zero(),
            play_start: TimeMus::zero(),
            meters: BTreeMap::new(),
            waveforms: BTreeMap::new(),
            xruns: 0,
            cpu_load: None,
            status: None,
            plugins: Vec::new(),
            bundle_path: None,
            dirty: false,
            saving: false,
            audio_settings: AudioSettingsView::default(),
            dsp: DspAnalysisView::default(),
            clipboard: Clipboard::default(),
        }
    }
}
