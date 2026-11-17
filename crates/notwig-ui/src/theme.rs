use egui::{Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(255, 166, 66);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(138, 86, 32);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 110, 90);
pub const PANEL: Color32 = Color32::from_rgb(23, 24, 29);
pub const PANEL_LIGHT: Color32 = Color32::from_rgb(31, 33, 39);
pub const LANE: Color32 = Color32::from_rgb(27, 29, 35);
pub const LANE_ALT: Color32 = Color32::from_rgb(22, 24, 29);
pub const GRID: Color32 = Color32::from_rgb(44, 47, 56);
pub const GRID_STRONG: Color32 = Color32::from_rgb(62, 66, 78);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(148, 153, 166);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(62, 142, 189);
pub const CLIP_MIDI: Color32 = Color32::from_rgb(126, 170, 96);
pub const SELECTION: Color32 = Color32::from_rgb(255, 210, 120);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL_LIGHT;
    visuals.extreme_bg_color = Color32::from_rgb(16, 17, 21);
    visuals.faint_bg_color = LANE;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 48, 57);
    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.window_corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.slider_width = 120.0;
    style.spacing.interact_size.y = 22.0;
    ctx.set_style(style);
}
