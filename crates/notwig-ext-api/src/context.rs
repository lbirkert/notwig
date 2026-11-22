use std::collections::BTreeMap;

use notwig_core::{ParamId, ParamValue};
use serde::{Deserialize, Serialize};

use crate::egui_panel::EguiInput;
use crate::geometry::Color;
use crate::ui::UiInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSnapshot {
    pub accent: Color,
    pub accent_dim: Color,
    pub playhead: Color,
    pub panel: Color,
    pub panel_light: Color,
    pub lane: Color,
    pub grid: Color,
    pub grid_strong: Color,
    pub text: Color,
    pub text_muted: Color,
    pub selection: Color,
    pub clip_audio: Color,
    pub clip_midi: Color,
}

impl Default for ThemeSnapshot {
    fn default() -> Self {
        Self {
            accent: Color::rgb(255, 166, 66),
            accent_dim: Color::rgb(138, 86, 32),
            playhead: Color::rgb(255, 110, 90),
            panel: Color::rgb(23, 24, 29),
            panel_light: Color::rgb(31, 33, 39),
            lane: Color::rgb(27, 29, 35),
            grid: Color::rgb(44, 47, 56),
            grid_strong: Color::rgb(62, 66, 78),
            text: Color::rgb(235, 237, 242),
            text_muted: Color::rgb(148, 153, 166),
            selection: Color::rgb(255, 210, 120),
            clip_audio: Color::rgb(62, 142, 189),
            clip_midi: Color::rgb(126, 170, 96),
        }
    }
}

/// Input and output peaks measured for one device during the latest block.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct DeviceMeterSnapshot {
    /// Left-channel peak before the device.
    pub input_peak_l: f32,
    /// Right-channel peak before the device.
    pub input_peak_r: f32,
    /// Left-channel peak after the device.
    pub output_peak_l: f32,
    /// Right-channel peak after the device.
    pub output_peak_r: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceDrawContext {
    pub device_id: String,
    pub type_id: String,
    pub name: String,
    pub sample_rate: u32,
    pub width: f32,
    pub height: f32,
    pub selected: bool,
    pub bypassed: bool,
    pub params: BTreeMap<ParamId, ParamValue>,
    pub theme: ThemeSnapshot,
    /// Pointer, modifier, and focus snapshot for the command-IR panel path.
    #[serde(default)]
    pub input: UiInput,
    /// Pointer, keyboard, and focus snapshot for the egui panel path.
    #[serde(default)]
    pub egui: EguiInput,
    /// Latest input/output peaks measured by the engine.
    #[serde(default)]
    pub meter: DeviceMeterSnapshot,
}
