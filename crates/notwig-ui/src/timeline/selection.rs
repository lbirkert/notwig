use egui::{Align2, Color32, FontId, Painter, Rect, Stroke};
use notwig_core::RangeMus;

use crate::timeline::TimelineGrid;

pub(crate) fn draw_time_selection_fill(
    painter: &Painter,
    grid: TimelineGrid,
    content_left: f32,
    rect: Rect,
    range: RangeMus,
) {
    let x1 = grid.x_for_beat(range.start, content_left).max(rect.left());
    let x2 = grid.x_for_beat(range.end, content_left).min(rect.right());
    if x2 > x1 {
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(x1, rect.top()), egui::pos2(x2, rect.bottom())),
            egui::CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(255, 210, 120, 34),
        );
    }
}

pub(crate) fn draw_time_selection_markers(
    painter: &Painter,
    grid: TimelineGrid,
    content_left: f32,
    marker_rect: Rect,
    label_rect: Rect,
    range: RangeMus,
) {
    for (beat, label) in [(range.start, "IN"), (range.end, "OUT")] {
        let x = grid.x_for_beat(beat, content_left);
        if x < marker_rect.left() - 1.0 || x > marker_rect.right() + 1.0 {
            continue;
        }
        painter.line_segment(
            [
                egui::pos2(x, marker_rect.top()),
                egui::pos2(x, marker_rect.bottom()),
            ],
            Stroke::new(1.5_f32, Color32::WHITE),
        );
        painter.text(
            egui::pos2(x + 3.0, label_rect.bottom() - 11.0),
            Align2::LEFT_TOP,
            label,
            FontId::monospace(8.0),
            Color32::WHITE,
        );
    }
}
