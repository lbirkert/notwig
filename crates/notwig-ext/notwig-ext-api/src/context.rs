use std::collections::BTreeMap;

use notwig_core::{ParamId, ParamValue};
use serde::{Deserialize, Serialize};

use crate::geometry::Color;
use crate::panel_frame::PanelInput;

/// Host theme colors copied into the guest for one panel frame.
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

/// Everything the host supplies for one device panel frame.
///
/// The guest receives this serialized before it draws and must not retain it
/// beyond the draw call. Cache per-instance UI state on the device, keyed by
/// [`device_id`](Self::device_id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceDrawContext {
    /// Stable identity of this device instance.
    pub device_id: String,
    /// Device type id.
    pub type_id: String,
    /// User-facing device name.
    pub name: String,
    /// Engine sample rate.
    pub sample_rate: u32,
    /// Panel width in points.
    pub width: f32,
    /// Panel height in points.
    pub height: f32,
    /// Whether the device is selected.
    pub selected: bool,
    /// Whether the device is bypassed.
    pub bypassed: bool,
    /// Current parameter values by id.
    pub params: BTreeMap<ParamId, ParamValue>,
    /// Active host theme.
    pub theme: ThemeSnapshot,
    /// Pointer, keyboard, and focus snapshot for the panel path.
    #[serde(default)]
    pub input: PanelInput,
    /// Latest input/output peaks measured by the engine.
    #[serde(default)]
    pub meter: DeviceMeterSnapshot,
}
