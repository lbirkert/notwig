//! Shared panel helpers for the stock extension devices.

use egui::{Align2, FontId};
use notwig_ext_api::egui;
use notwig_ui::palette;

/// Paints the small caption under a knob cell.
pub(crate) fn knob_label(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(rect.center().x, rect.bottom() + 8.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(10.0),
        palette::TEXT_MUTED,
    );
}
