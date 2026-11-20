use egui::{
    Color32, CornerRadius, FontFamily, FontId, Painter, Rect, Response, Sense, Stroke, StrokeKind,
    Vec2,
};

use crate::theme;

/// Icon glyphs from the bundled Lucide icon font.
///
/// Do not replace these with hand-painted shapes. If a new icon is needed,
/// add its Lucide name and Unicode code point from the font's cmap.
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
    Record,
    Metronome,
    Audio,
    Instrument,
    Menu,
    ExternalLink,
    Search,
    Sliders,
    Clip,
    MoveHorizontal,
}

impl Icon {
    fn glyph(self) -> char {
        match self {
            Self::Play => '\u{e13c}',                   // play
            Self::Pause => '\u{e12e}',                  // pause
            Self::Stop => '\u{e167}',                   // square
            Self::Loop => '\u{e146}',                   // repeat
            Self::Power => '\u{e140}',                  // power
            Self::Fit => '\u{e113}',                    // maximize-2
            Self::Plus => '\u{e13d}',                   // plus
            Self::Minus => '\u{e11c}',                  // minus
            Self::Render => '\u{e0b2}',                 // download
            Self::Help => '\u{e082}',                   // circle-help
            Self::Info => '\u{e0f9}',                   // info
            Self::Device | Self::Sliders => '\u{e29a}', // sliders-horizontal
            Self::Folder => '\u{e0d7}',                 // folder
            Self::Save => '\u{e14d}',                   // save
            Self::Settings => '\u{e154}',               // settings
            Self::Record => '\u{e076}',                 // circle
            Self::Metronome => '\u{e6bc}',              // metronome
            Self::Audio => '\u{e55a}',                  // audio-lines
            Self::Instrument => '\u{e561}',             // piano
            Self::Menu => '\u{e0b6}',                   // ellipsis
            Self::ExternalLink => '\u{e0b9}',           // external-link
            Self::Search => '\u{e151}',                 // search
            Self::Clip => '\u{e29b}',                   // clapperboard
            Self::MoveHorizontal => '\u{e1c6}',         // move-horizontal
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
    // Keep the glyph well inside the button so icon buttons read with clear
    // inner padding rather than filling their frame edge to edge.
    let size = (rect.width().min(rect.height()) * 0.56).clamp(7.0, 24.0);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon.glyph(),
        FontId::new(size, FontFamily::Proportional),
        color,
    );
}
