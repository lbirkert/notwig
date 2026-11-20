use std::path::Path;

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind};

use crate::theme;

pub(crate) fn draw_drop_preview(painter: &Painter, rect: Rect, path: &Path, analyzing: bool) {
    painter.rect_filled(
        rect,
        CornerRadius::same(3),
        theme::ACCENT.gamma_multiply(0.28),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.5_f32, theme::ACCENT),
        StrokeKind::Inside,
    );
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        let label = if analyzing {
            format!("Analyzing {name}…")
        } else {
            name.to_owned()
        };
        painter.with_clip_rect(rect).text(
            egui::pos2(rect.left() + 5.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::monospace(10.0),
            Color32::WHITE,
        );
    }
}
