use std::collections::BTreeMap;

use notwig_core::{ParamId, ParamValue};
use serde::{Deserialize, Serialize};

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
    /// Pointer, modifier, and focus snapshot for this frame.
    #[serde(default)]
    pub input: UiInput,
}
