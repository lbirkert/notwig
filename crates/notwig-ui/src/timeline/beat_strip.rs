use egui::{Align2, Color32, FontId, Id, Painter, Rect, Sense, Stroke, Ui};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;

pub struct BeatStrip<'a> {
    pub grid: TimelineGrid,
    pub header_width: f32,
    pub header_label: Option<&'a str>,
    pub loop_range: Option<RangeMus>,
    pub time_selection: Option<RangeMus>,
    pub play_start: TimeMus,
    pub playhead: TimeMus,
    pub show_play_start: bool,
}

impl BeatStrip<'_> {
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> Option<TimeMus> {
        let content_left = rect.left() + self.header_width;
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::PANEL_LIGHT);

        if self.header_width > 0.0 {
            painter.line_segment(
                [
                    egui::pos2(content_left, rect.top()),
                    egui::pos2(content_left, rect.bottom()),
                ],
                Stroke::new(1.0_f32, theme::GRID_STRONG),
            );
            if let Some(label) = self.header_label {
                painter.text(
                    egui::pos2(rect.left() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(11.0),
                    theme::TEXT_MUTED,
                );
            }
        }

        if let Some(range) = self.loop_range {
            fill_range(
                &painter,
                rect,
                content_left,
                self.grid,
                range,
                theme::ACCENT.gamma_multiply(0.18),
            );
        }
        if let Some(range) = self.time_selection {
            fill_range(
                &painter,
                rect,
                content_left,
                self.grid,
                range,
                Color32::from_rgba_unmultiplied(255, 210, 120, 34),
            );
        }

        draw_beats(&painter, rect, content_left, self.grid);
        if self.show_play_start {
            draw_play_start(&painter, rect, content_left, self.grid, self.play_start);
        }
        draw_playhead(&painter, rect, content_left, self.grid, self.playhead);

        let response = ui.interact(rect, id, Sense::click_and_drag());
        if response.clicked() || response.dragged() {
            response
                .interact_pointer_pos()
                .filter(|pos| pos.x > content_left)
                .map(|pos| {
                    let raw = self.grid.beat_for_x(pos.x, content_left);
                    let bypass = ui.input(|input| input.modifiers.shift);
                    self.grid.snap(raw, bypass)
                })
        } else {
            None
        }
    }
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

fn fill_range(
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

fn draw_beats(painter: &Painter, rect: Rect, content_left: f32, grid: TimelineGrid) {
    let visible_beats = f64::from(((rect.right() - content_left) / grid.pixels_per_beat).max(1.0));
    let step = grid.step.as_f64().max(1.0 / 256.0);
    let bar_len = f64::from(grid.beats_per_bar);
    let show_beats = grid.pixels_per_beat >= 26.0;
    let first = (grid.start.as_f64() / step).floor() as i64;
    let last = ((grid.start.as_f64() + visible_beats) / step).ceil() as i64 + 1;
    for index in first..=last {
        let beat = index as f64 * step;
        if beat < 0.0 {
            continue;
        }
        let x = content_left + (beat - grid.start.as_f64()) as f32 * grid.pixels_per_beat;
        if x < content_left - 1.0 || x > rect.right() + 1.0 {
            continue;
        }
        let strong = (beat / bar_len).fract().abs() < 1e-6;
        let beat_line = beat.fract().abs() < 1e-6;
        painter.line_segment(
            [
                egui::pos2(
                    x,
                    rect.top()
                        + if strong {
                            6.0
                        } else if beat_line {
                            14.0
                        } else {
                            20.0
                        },
                ),
                egui::pos2(x, rect.bottom()),
            ],
            Stroke::new(
                1.0_f32,
                if strong {
                    theme::GRID_STRONG
                } else if beat_line {
                    theme::GRID
                } else {
                    theme::GRID.gamma_multiply(0.55)
                },
            ),
        );
        if strong || (show_beats && beat_line) {
            let bar = (beat / bar_len).floor() as i64 + 1;
            let beat_in_bar = (beat % bar_len).floor() as i64 + 1;
            painter.text(
                egui::pos2(x + 3.0, rect.top() + 3.0),
                Align2::LEFT_TOP,
                format!("{bar}.{beat_in_bar}"),
                FontId::monospace(9.0),
                if strong {
                    theme::TEXT_MUTED
                } else {
                    theme::GRID_STRONG
                },
            );
        }
    }
}

fn draw_play_start(
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
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x, rect.top() + 4.0),
            egui::pos2(x, rect.top() + 14.0),
            egui::pos2(x + 8.0, rect.top() + 9.0),
        ],
        theme::ACCENT,
        Stroke::NONE,
    ));
}
