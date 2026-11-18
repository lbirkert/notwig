use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind};

use crate::theme;
use crate::widgets::format_db;

/// Default decibel range for track volume faders.
pub(crate) const DB_MIN: f32 = -60.0;
pub(crate) const DB_MAX: f32 = 6.0;

/// Draws a horizontal decibel fader and handles drag interaction.
///
/// Returns the new decibel value when it changed.
pub(crate) fn volume_fader(
    ui: &mut egui::Ui,
    id: egui::Id,
    rect: Rect,
    db: f32,
    min_db: f32,
    max_db: f32,
) -> Option<f32> {
    let response = ui
        .interact(rect, id, Sense::click_and_drag())
        .on_hover_text(format_db(notwig_core::Db(db)));
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let value = crate::widgets::value_drag(
        ui,
        &response,
        id,
        f64::from(db),
        0.0,
        f64::from(min_db),
        f64::from(max_db),
        f64::from(max_db - min_db) / 200.0,
        false,
    )
    .map_or(db, |value| value as f32);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(15, 16, 20));
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0_f32, theme::GRID),
        StrokeKind::Inside,
    );
    let t = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);
    let fill = Rect::from_min_max(
        rect.min,
        egui::pos2(rect.left() + rect.width() * t, rect.bottom()),
    );
    painter.rect_filled(fill, CornerRadius::same(3), Color32::from_rgb(116, 67, 25));
    if t > 0.0 {
        painter.line_segment(
            [
                egui::pos2(fill.right().max(rect.left() + 1.0), rect.top() + 2.0),
                egui::pos2(fill.right().max(rect.left() + 1.0), rect.bottom() - 2.0),
            ],
            Stroke::new(1.5_f32, Color32::from_rgb(255, 145, 46)),
        );
    }
    let zero_t = (0.0 - min_db) / (max_db - min_db);
    let zero_x = rect.left() + rect.width() * zero_t;
    painter.line_segment(
        [
            egui::pos2(zero_x, rect.top() + 3.0),
            egui::pos2(zero_x, rect.bottom() - 3.0),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(50)),
    );
    (value - db).abs().gt(&1e-9_f32).then_some(value)
}
