//! Shared panel helpers for the stock extension devices.

use egui::{Align2, FontId, Sense, Vec2};
use notwig_ext_api::egui;
use notwig_ui::{knob, palette};

/// Paints the small caption under a knob cell.
pub(crate) fn knob_label(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    let clip = rect.expand2(egui::vec2(0.0, 18.0));
    ui.painter().with_clip_rect(clip).text(
        egui::pos2(rect.center().x, rect.bottom() + 8.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(10.0),
        palette::TEXT_MUTED,
    );
}

/// Allocates a compact knob cell and paints its caption.
pub(crate) fn knob_cell(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    value: f32,
    default: f32,
    bipolar: bool,
    width: f32,
    height: f32,
) -> Option<f32> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let changed = knob(ui, id, rect, value, default, bipolar, label);
    knob_label(ui, rect, label);
    changed
}
