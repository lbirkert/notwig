#![forbid(unsafe_code)]

//! Shared color palette used by host and extension-facing components.

use egui::Color32;

pub const ACCENT: Color32 = Color32::from_rgb(255, 166, 66);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(138, 86, 32);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 110, 90);
pub const PLAY_START: Color32 = Color32::from_rgb(126, 190, 255);
pub const PANEL: Color32 = Color32::from_rgb(23, 24, 29);
pub const PANEL_LIGHT: Color32 = Color32::from_rgb(31, 33, 39);
pub const LANE: Color32 = Color32::from_rgb(27, 29, 35);
pub const ARRANGER_BG: Color32 = Color32::from_rgb(32, 35, 42);
pub const GRID: Color32 = Color32::from_rgb(44, 47, 56);
pub const GRID_STRONG: Color32 = Color32::from_rgb(68, 73, 86);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(235, 238, 245);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(158, 164, 177);
pub const HEADING: Color32 = Color32::from_rgb(241, 244, 249);
pub const APP_BAR: Color32 = Color32::from_rgb(20, 21, 26);
pub const APP_BAR_CARD: Color32 = Color32::from_rgb(37, 40, 48);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(62, 142, 189);
pub const CLIP_MIDI: Color32 = Color32::from_rgb(126, 170, 96);
pub const SELECTION: Color32 = Color32::from_rgb(255, 210, 120);
