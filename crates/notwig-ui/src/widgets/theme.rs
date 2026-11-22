use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
    TextStyle,
};

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

pub const HEADING_SIZE: f32 = 11.0;
pub const APP_TITLE_SIZE: f32 = 18.0;

const INTER_FAMILIES: [(&str, &str); 5] = [
    ("inter_regular", "Inter Regular"),
    ("inter_medium", "Inter Medium"),
    ("inter_semibold", "Inter SemiBold"),
    ("inter_bold", "Inter Bold"),
    ("inter_black", "Inter Black"),
];

pub fn app_title_font(ctx: &egui::Context, size: f32) -> FontId {
    custom_font(ctx, "Inter Black", size)
}

pub fn heading_font(ctx: &egui::Context) -> FontId {
    custom_font(ctx, "Inter SemiBold", HEADING_SIZE)
}

/// Standard uppercase section heading used across panels.
pub fn heading(ctx: &egui::Context, label: impl Into<String>) -> RichText {
    RichText::new(label).font(heading_font(ctx)).color(HEADING)
}

/// Page-level heading used by settings and other full-page views.
pub fn page_heading(ctx: &egui::Context, label: impl Into<String>) -> RichText {
    RichText::new(label)
        .font(custom_font(ctx, "Inter Bold", 18.0))
        .color(TEXT_PRIMARY)
}

/// Icon glyphs must always resolve to Lucide. Inter has private-use glyphs at
/// the same code points, so the icon font needs its own family.
pub fn icon_font(ctx: &egui::Context, size: f32) -> FontId {
    let named = FontFamily::Name("Lucide".into());
    if ctx.fonts(|fonts| fonts.families().contains(&named)) {
        FontId::new(size, named)
    } else {
        FontId::new(size, FontFamily::Proportional)
    }
}

/// Named font families are installed asynchronously by egui. During the first
/// frame (or in tests that never configure the context) fall back to the
/// proportional family so rendering never depends on install timing.
pub fn custom_font(ctx: &egui::Context, family: &str, size: f32) -> FontId {
    let named = FontFamily::Name(family.into());
    if ctx.fonts(|fonts| fonts.families().contains(&named)) {
        FontId::new(size, named)
    } else {
        FontId::proportional(size)
    }
}

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
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 52, 62);
    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.window_corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.slider_width = 120.0;
    style.spacing.interact_size.y = 22.0;
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(10.0, FontFamily::Proportional),
    );
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
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (key, bytes) in [
        (
            "inter_regular",
            include_bytes!("../../assets/fonts/Inter-Regular.ttf").as_slice(),
        ),
        (
            "inter_medium",
            include_bytes!("../../assets/fonts/Inter-Medium.ttf").as_slice(),
        ),
        (
            "inter_semibold",
            include_bytes!("../../assets/fonts/Inter-SemiBold.ttf").as_slice(),
        ),
        (
            "inter_bold",
            include_bytes!("../../assets/fonts/Inter-Bold.ttf").as_slice(),
        ),
        (
            "inter_black",
            include_bytes!("../../assets/fonts/Inter-Black.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(key.into(), FontData::from_static(bytes).into());
    }
    fonts.font_data.insert(
        "lucide".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/lucide.ttf")).into(),
    );

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let family_fonts = fonts.families.entry(family).or_default();
        family_fonts.insert(0, "inter_regular".into());
        // Keep Lucide available as a fallback for icon glyphs in text.
        family_fonts.insert(1, "lucide".into());
    }
    // Icons use this family directly because Inter also maps private-use
    // code points, which would otherwise render the wrong glyphs.
    fonts
        .families
        .insert(FontFamily::Name("Lucide".into()), vec!["lucide".into()]);

    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (key, name) in INTER_FAMILIES {
        let mut family = vec![key.into()];
        family.extend(fallback.iter().filter(|font| *font != key).cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }

    ctx.set_fonts(fonts);
}
