#![forbid(unsafe_code)]

//! Shared color palette used by host and extension-facing components.

use egui::Color32;

pub const ACCENT: Color32 = Color32::from_rgb(255, 154, 46);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(128, 74, 22);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 122, 92);
pub const PLAY_START: Color32 = Color32::from_rgb(120, 180, 255);
pub const PANEL: Color32 = Color32::from_rgb(28, 28, 30);
pub const PANEL_LIGHT: Color32 = Color32::from_rgb(38, 38, 41);
pub const LANE: Color32 = Color32::from_rgb(42, 42, 45);
pub const ARRANGER_BG: Color32 = Color32::from_rgb(54, 54, 57);
pub const GRID: Color32 = Color32::from_rgb(66, 66, 70);
pub const GRID_STRONG: Color32 = Color32::from_rgb(86, 86, 92);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 228, 232);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(150, 150, 158);
pub const HEADING: Color32 = Color32::from_rgb(240, 240, 243);
pub const APP_BAR: Color32 = Color32::from_rgb(20, 20, 22);
pub const APP_BAR_CARD: Color32 = Color32::from_rgb(45, 45, 49);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(198, 158, 66);
pub const CLIP_MIDI: Color32 = Color32::from_rgb(104, 140, 116);
pub const SELECTION: Color32 = Color32::from_rgb(255, 208, 122);
