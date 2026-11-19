use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Painter, Rect, Sense, Stroke, StrokeKind, Ui,
};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;

const LOOP_EDGE_WIDTH: f32 = 7.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeatStripAction {
    Locate(TimeMus),
    SetLoop(RangeMus),
}

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
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> Option<BeatStripAction> {
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

        let mut action = None;
        if let Some(range) = self.loop_range {
            let loop_rect = loop_rect(rect, content_left, self.grid, range);
            draw_loop(&painter, loop_rect);
            if let Some(next) = self.loop_edge_action(ui, rect, id, content_left, range) {
                action = Some(BeatStripAction::SetLoop(next));
            }
        }

        if self.show_play_start {
            draw_play_start(&painter, rect, content_left, self.grid, self.play_start);
        }
        draw_playhead(&painter, rect, content_left, self.grid, self.playhead);

        let response = ui.interact(rect, id, Sense::click_and_drag());
        if action.is_none() && (response.clicked() || response.dragged()) {
            action = response
                .interact_pointer_pos()
                .filter(|pos| pos.x > content_left)
                .map(|pos| {
                    let raw = self.grid.beat_for_x(pos.x, content_left);
                    let bypass = ui.input(|input| input.modifiers.shift);
                    BeatStripAction::Locate(self.grid.snap(raw, bypass))
                });
        }
        action
    }

    fn loop_edge_action(
        &self,
        ui: &mut Ui,
        rect: Rect,
        id: Id,
        content_left: f32,
        range: RangeMus,
    ) -> Option<RangeMus> {
        let loop_rect = loop_rect(rect, content_left, self.grid, range);
        let mut next = None;
        for (edge, x) in [
            (LoopEdge::Start, loop_rect.left()),
            (LoopEdge::End, loop_rect.right()),
        ] {
            let handle = Rect::from_min_max(
                egui::pos2(x - LOOP_EDGE_WIDTH * 0.5, loop_rect.top()),
                egui::pos2(x + LOOP_EDGE_WIDTH * 0.5, loop_rect.bottom()),
            )
            .intersect(loop_rect);
            if handle.width() < 1.0 {
                continue;
            }
            let response = ui.interact(
                handle,
                id.with(match edge {
                    LoopEdge::Start => "loop-start",
                    LoopEdge::End => "loop-end",
                }),
                Sense::drag(),
            );
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if response.drag_stopped()
                && let Some(pos) = response
                    .interact_pointer_pos()
                    .or_else(|| ui.input(|input| input.pointer.hover_pos()))
            {
                let raw = self.grid.beat_for_x(pos.x, content_left);
                let bypass = ui.input(|input| input.modifiers.shift);
                let beat = self.grid.snap(raw, bypass);
                if let Some(candidate) = loop_edge_range(edge, range, beat) {
                    next = Some(candidate);
                }
            }
        }
        next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopEdge {
    Start,
    End,
}

pub(super) fn loop_edge_range(edge: LoopEdge, range: RangeMus, beat: TimeMus) -> Option<RangeMus> {
    let candidate = match edge {
        LoopEdge::Start => RangeMus::new(beat, range.end).ok()?,
        LoopEdge::End => RangeMus::new(range.start, beat).ok()?,
    };
    (!candidate.is_empty()).then_some(candidate)
}

fn loop_rect(rect: Rect, content_left: f32, grid: TimelineGrid, range: RangeMus) -> Rect {
    let top = rect.top() + 17.0;
    let bottom = rect.bottom() - 1.0;
    let x1 = grid
        .x_for_beat(range.start, content_left)
        .max(content_left)
        .min(rect.right());
    let x2 = grid
        .x_for_beat(range.end, content_left)
        .max(content_left)
        .min(rect.right());
    Rect::from_min_max(
        egui::pos2(x1, top.min(bottom)),
        egui::pos2(x2.max(x1 + 1.0), bottom),
    )
}

fn draw_loop(painter: &Painter, rect: Rect) {
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
