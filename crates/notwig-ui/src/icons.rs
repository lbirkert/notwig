use egui::{
    Color32, CornerRadius, FontFamily, FontId, Painter, Rect, Response, Sense, Stroke, StrokeKind,
    Vec2,
};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Loop,
    Power,
    Fit,
    Plus,
    Minus,
    Render,
    Help,
    Info,
    Device,
    Folder,
    Save,
    Settings,
}

impl Icon {
    fn glyph(self) -> &'static str {
        match self {
            Self::Play => "\u{e13c}",
            Self::Pause => "\u{e12e}",
            Self::Stop => "\u{e167}",
            Self::Loop => "\u{e146}",
            Self::Power => "\u{e140}",
            Self::Fit => "\u{e112}",
            Self::Plus => "\u{e13d}",
            Self::Minus => "\u{e11c}",
            Self::Render => "\u{e0b2}",
            Self::Help => "\u{e082}",
            Self::Info => "\u{e0f9}",
            Self::Device => "\u{e29a}",
            Self::Folder => "\u{e0d7}",
            Self::Save => "\u{e14d}",
            Self::Settings => "\u{e8b8}",
        }
    }
}

pub fn icon_button_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    size: Vec2,
    selected: bool,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        theme::ACCENT_DIM
    } else if response.hovered() {
        theme::GRID_STRONG
    } else {
        theme::PANEL
    };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, if selected { theme::ACCENT } else { theme::GRID }),
        StrokeKind::Inside,
    );
    paint(
        painter,
        rect,
        icon,
        if selected {
            theme::ACCENT
        } else {
            Color32::WHITE
        },
    );
    response.on_hover_text(tooltip)
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon.glyph(),
        FontId::new(
            (rect.height() * 0.72).clamp(12.0, 20.0),
            FontFamily::Monospace,
        ),
        color,
    );
}
