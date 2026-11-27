use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;

use crate::widgets::timeline_grid::{aligned, line_indices};

pub(super) fn draw_loop(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(66, 69, 77));
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::WHITE),
        StrokeKind::Inside,
    );
}

pub fn draw_playhead(
    painter: &Painter,
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    playhead: TimeMus,
) {
    let x = grid.x_for_beat(playhead, content_left);
    if x < rect.left() || x > rect.right() {
        return;
    }
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        Stroke::new(1.0_f32, Color32::WHITE),
    );
}

pub(super) fn fill_range(
    painter: &Painter,
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    range: RangeMus,
    color: Color32,
) {
    let x1 = grid.x_for_beat(range.start, content_left).max(rect.left());
    let x2 = grid.x_for_beat(range.end, content_left).min(rect.right());
    if x2 > x1 {
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(x1, rect.top()), egui::pos2(x2, rect.bottom())),
            egui::CornerRadius::ZERO,
            color,
        );
    }
}

pub(super) fn draw_beats(painter: &Painter, rect: Rect, content_left: f32, grid: TimelineGrid) {
    let visible_beats = f64::from(((rect.right() - content_left) / grid.pixels_per_beat).max(1.0));
    let start = grid.start.as_f64();
    let end = start + visible_beats;
    let bar = grid.bar_length.as_f64().max(1e-9);
    let beat_length = grid.beat_length.as_f64().max(1e-9);
    let step = grid.step.as_f64();
    let show_beats = grid.pixels_per_beat >= 26.0;

    if step > 0.0 && step as f32 * grid.pixels_per_beat >= 1.0 {
        for index in line_indices(start, end, step) {
            let beat = index as f64 * step;
            if beat < 0.0 || aligned(beat, bar) || aligned(beat, beat_length) {
                continue;
            }
            draw_beat_line(painter, rect, content_left, grid, beat, 20.0, theme::GRID);
        }
    }

    for index in line_indices(start, end, beat_length) {
        let beat = index as f64 * beat_length;
        if beat < 0.0 || aligned(beat, bar) {
            continue;
        }
        draw_beat_line(
            painter,
            rect,
            content_left,
            grid,
            beat,
            14.0,
            theme::GRID_STRONG,
        );
        if show_beats {
            let bar_index = (beat / bar).floor() as i64 + 1;
            let beat_in_bar =
                ((beat - (bar_index - 1) as f64 * bar) / beat_length).round() as i64 + 1;
            draw_beat_label(
                painter,
                rect,
                content_left,
                grid,
                beat,
                format!("{bar_index}.{beat_in_bar}"),
                theme::GRID_STRONG,
            );
        }
    }

    for index in line_indices(start, end, bar) {
        let beat = index as f64 * bar;
        if beat < 0.0 {
            continue;
        }
        draw_beat_line(
            painter,
            rect,
            content_left,
            grid,
            beat,
            6.0,
            egui::Color32::from_rgb(92, 97, 112),
        );
        draw_beat_label(
            painter,
            rect,
            content_left,
            grid,
            beat,
            format!("{}.1", index + 1),
            theme::TEXT_MUTED,
        );
    }
}

fn draw_beat_line(
    painter: &Painter,
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    beat: f64,
    top_inset: f32,
    color: Color32,
) {
    let x = content_left + (beat - grid.start.as_f64()) as f32 * grid.pixels_per_beat;
    if x < content_left - 1.0 || x > rect.right() + 1.0 {
        return;
    }
    painter.line_segment(
        [
            egui::pos2(x, rect.top() + top_inset),
            egui::pos2(x, rect.bottom()),
        ],
        Stroke::new(
            1.0_f32,
            crate::timeline::grid_color(color, grid.snap_enabled),
        ),
    );
}

fn draw_beat_label(
    painter: &Painter,
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    beat: f64,
    label: String,
    color: Color32,
) {
    let x = content_left + (beat - grid.start.as_f64()) as f32 * grid.pixels_per_beat;
    if x < content_left - 1.0 || x > rect.right() + 1.0 {
        return;
    }
    painter.text(
        egui::pos2(x + 3.0, rect.top() + 3.0),
        Align2::LEFT_TOP,
        label,
        FontId::monospace(9.0),
        crate::timeline::grid_color(color, grid.snap_enabled),
    );
}

pub(super) fn draw_play_start(
    painter: &Painter,
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    play_start: TimeMus,
) {
    let x = grid.x_for_beat(play_start, content_left);
    if x < content_left || x > rect.right() {
        return;
    }
    let center_y = rect.top() + 7.0;
    let half_height = 5.0;
    let width = 9.0;
    let color = theme::PLAY_START;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x, center_y - half_height),
            egui::pos2(x, center_y + half_height),
            egui::pos2(x + width, center_y),
        ],
        color,
        Stroke::new(1.0_f32, color.gamma_multiply(0.62)),
    ));
}
