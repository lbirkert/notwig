use std::collections::BTreeMap;
use std::path::PathBuf;

use notwig_core::{DeviceKind, TimeMus, TrackId};

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

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeView {
    pub playing: bool,
    pub playhead: TimeMus,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub status: Option<String>,
    pub plugins: Vec<PluginEntry>,
    pub bundle_path: Option<PathBuf>,
    pub dirty: bool,
    pub saving: bool,
}

impl Default for RuntimeView {
    fn default() -> Self {
        Self {
            playing: false,
            playhead: TimeMus::zero(),
            meters: BTreeMap::new(),
            xruns: 0,
            cpu_load: None,
            status: None,
            plugins: Vec::new(),
            bundle_path: None,
            dirty: false,
            saving: false,
        }
    }
}
