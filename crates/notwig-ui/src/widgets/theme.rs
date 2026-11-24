use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
    TextStyle,
};

pub use crate::palette::*;

pub const HEADING_SIZE: f32 = 11.0;
pub const APP_TITLE_SIZE: f32 = 14.0;

const GEIST_FAMILIES: [(&str, &str); 5] = [
    ("geist_regular", "Geist"),
    ("geist_medium", "Geist Medium"),
    ("geist_semibold", "Geist SemiBold"),
    ("geist_bold", "Geist Bold"),
    ("geist_black", "Geist Black"),
];

pub fn app_title_font(ctx: &egui::Context, size: f32) -> FontId {
    custom_font(ctx, "Geist Black", size)
}

pub fn heading_font(ctx: &egui::Context) -> FontId {
    custom_font(ctx, "Geist SemiBold", HEADING_SIZE)
}

/// Standard uppercase section heading used across panels.
pub fn heading(ctx: &egui::Context, label: impl Into<String>) -> RichText {
    RichText::new(label).font(heading_font(ctx)).color(HEADING)
}

/// Page-level heading used by settings and other full-page views.
pub fn page_heading(ctx: &egui::Context, label: impl Into<String>) -> RichText {
    RichText::new(label)
        .font(custom_font(ctx, "Geist Bold", 18.0))
        .color(TEXT_PRIMARY)
}

/// Icon glyphs must always resolve to Lucide. The text fonts may map
/// private-use glyphs at the same code points, so icons need their own family.
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
    visuals.window_stroke = Stroke::new(1.0_f32, GRID_STRONG);
    visuals.extreme_bg_color = Color32::from_rgb(16, 16, 18);
    visuals.faint_bg_color = LANE;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
    visuals.widgets.inactive.weak_bg_fill = PANEL_LIGHT;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(54, 54, 60);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(54, 54, 60);
    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.active.weak_bg_fill = ACCENT_DIM;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    // Buttons and combo boxes share the same rounded, outlined treatment so a
    // dropdown reads as a button that opens a list.
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = CornerRadius::same(5);
    }
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, GRID);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, GRID_STRONG);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    // The title bar of default egui windows uses this fill when the window
    // frame is not overridden, so it matches the application header.
    visuals.widgets.open.weak_bg_fill = APP_BAR;
    visuals.widgets.open.corner_radius = CornerRadius::same(5);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, GRID_STRONG);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.window_corner_radius = CornerRadius::same(6);
    visuals.menu_corner_radius = CornerRadius::same(4);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.slider_width = 120.0;
    style.spacing.interact_size.y = 24.0;
    style.spacing.combo_width = 180.0;
    // Compact window chrome; the header color is set through
    // `widgets.open.weak_bg_fill` above.
    style.spacing.window_margin = egui::Margin::symmetric(8, 4);
    style.spacing.menu_margin = egui::Margin::symmetric(6, 4);
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
            "geist_regular",
            include_bytes!("../../assets/fonts/Geist-Regular.ttf").as_slice(),
        ),
        (
            "geist_medium",
            include_bytes!("../../assets/fonts/Geist-Medium.ttf").as_slice(),
        ),
        (
            "geist_semibold",
            include_bytes!("../../assets/fonts/Geist-SemiBold.ttf").as_slice(),
        ),
        (
            "geist_bold",
            include_bytes!("../../assets/fonts/Geist-Bold.ttf").as_slice(),
        ),
        (
            "geist_black",
            include_bytes!("../../assets/fonts/Geist-Black.ttf").as_slice(),
        ),
        (
            "geist_mono_regular",
            include_bytes!("../../assets/fonts/GeistMono-Regular.ttf").as_slice(),
        ),
        (
            "geist_mono_medium",
            include_bytes!("../../assets/fonts/GeistMono-Medium.ttf").as_slice(),
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

    let proportional_fonts = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional_fonts.insert(0, "geist_regular".into());
    // Keep Lucide available as a fallback for icon glyphs in text.
    proportional_fonts.insert(1, "lucide".into());

    let monospace_fonts = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace_fonts.insert(0, "geist_mono_regular".into());
    monospace_fonts.insert(1, "geist_mono_medium".into());
    monospace_fonts.insert(2, "lucide".into());

    // Icons use this family directly because text fonts may map private-use
    // code points, which would otherwise render the wrong glyphs.
    fonts
        .families
        .insert(FontFamily::Name("Lucide".into()), vec!["lucide".into()]);

    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (key, name) in GEIST_FAMILIES {
        let mut family = vec![key.into()];
        family.extend(fallback.iter().filter(|font| *font != key).cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }

    ctx.set_fonts(fonts);
}
