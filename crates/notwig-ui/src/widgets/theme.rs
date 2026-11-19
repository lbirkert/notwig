use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

pub const ACCENT: Color32 = Color32::from_rgb(255, 166, 66);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(138, 86, 32);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 110, 90);
pub const PANEL: Color32 = Color32::from_rgb(23, 24, 29);
pub const PANEL_LIGHT: Color32 = Color32::from_rgb(31, 33, 39);
pub const LANE: Color32 = Color32::from_rgb(27, 29, 35);
pub const GRID: Color32 = Color32::from_rgb(44, 47, 56);
pub const GRID_STRONG: Color32 = Color32::from_rgb(62, 66, 78);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(148, 153, 166);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(62, 142, 189);
pub const CLIP_MIDI: Color32 = Color32::from_rgb(126, 170, 96);
pub const SELECTION: Color32 = Color32::from_rgb(255, 210, 120);

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
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
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(10.0, FontFamily::Monospace));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Monospace));
    ctx.set_style(style);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "jetbrains_mono".into(),
        FontData::from_static(include_bytes!(
            "../../assets/fonts/JetBrainsMono-Regular.ttf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        "jetbrains_mono_bold".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf")).into(),
    );
    fonts.font_data.insert(
        "lucide".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/lucide.ttf")).into(),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let family_fonts = fonts.families.entry(family).or_default();
        family_fonts.insert(0, "jetbrains_mono".into());
        // Put Lucide ahead of egui's bundled emoji/icon font so private-use
        // code points always resolve to the real Lucide glyphs.
        family_fonts.insert(1, "lucide".into());
    }
    ctx.set_fonts(fonts);
}
