use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Painter, Rect, Sense, Stroke, StrokeKind, Ui,
};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;

use super::grid::{aligned, line_indices};

const LOOP_EDGE_WIDTH: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeatStripAction {
    Locate(TimeMus),
    SetLoop(RangeMus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopDragMode {
    Start,
    End,
    Body,
}

#[derive(Debug, Clone, Copy)]
struct LoopDrag {
    mode: LoopDragMode,
    origin: RangeMus,
    start_pointer: egui::Pos2,
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
    /// Space kept free at the bottom of the strip for clip arrow tips.
    pub loop_bottom_inset: f32,
}

impl BeatStrip<'_> {
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> Option<BeatStripAction> {
        let content_left = rect.left() + self.header_width;
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::PANEL_LIGHT);
        let response = ui.interact(rect, id, Sense::click_and_drag());

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
                    theme::heading_font(ui.ctx()),
                    theme::HEADING,
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
        let mut loop_preview = self.loop_range;
        let mut loop_dragging = false;
        if let Some(range) = self.loop_range {
            let (preview, committed) = self.loop_interaction(ui, rect, id, content_left, range);
            loop_dragging = preview.is_some();
            if let Some(preview) = preview {
                loop_preview = Some(preview);
            }
            if let Some(committed) = committed {
                action = Some(BeatStripAction::SetLoop(committed));
            }
        }

        if let Some(range) = loop_preview {
            let loop_rect = loop_rect(rect, content_left, self.grid, range, self.loop_bottom_inset);
            draw_loop(&painter, loop_rect);
        }

        if self.show_play_start {
            draw_play_start(&painter, rect, content_left, self.grid, self.play_start);
        }
        draw_playhead(&painter, rect, content_left, self.grid, self.playhead);

        if action.is_none()
            && !loop_dragging
            && (response.clicked() || response.dragged())
            && !response
                .interact_pointer_pos()
                .is_some_and(|pos| self.pointer_inside_loop(rect, content_left, pos))
        {
            action = response
                .interact_pointer_pos()
                .filter(|pos| pos.x > content_left)
                .map(|pos| {
                    let raw = self.grid.beat_for_x(pos.x, content_left);
                    let bypass = ui.input(|input| self.grid.bypassed(input.modifiers));
                    BeatStripAction::Locate(self.grid.snap(raw, bypass))
                });
        }
        action
    }

    fn pointer_inside_loop(&self, rect: Rect, content_left: f32, pos: egui::Pos2) -> bool {
        self.loop_range.is_some_and(|range| {
            loop_rect(rect, content_left, self.grid, range, self.loop_bottom_inset).contains(pos)
        })
    }

    fn loop_interaction(
        &self,
        ui: &mut Ui,
        rect: Rect,
        id: Id,
        content_left: f32,
        range: RangeMus,
    ) -> (Option<RangeMus>, Option<RangeMus>) {
        let drag_key = id.with("loop-drag");
        let mut drag: Option<LoopDrag> = ui.data(|data| data.get_temp::<LoopDrag>(drag_key));
        let loop_rect = loop_rect(rect, content_left, self.grid, range, self.loop_bottom_inset);
        let body_rect = Rect::from_min_max(
            egui::pos2(loop_rect.left() + LOOP_EDGE_WIDTH * 0.5, loop_rect.top()),
            egui::pos2(
                loop_rect.right() - LOOP_EDGE_WIDTH * 0.5,
                loop_rect.bottom(),
            ),
        );

        let mut body_response = None;
        if body_rect.width() > 1.0 {
            let response = ui.interact(body_rect, id.with("loop-body"), Sense::click_and_drag());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                drag = Some(LoopDrag {
                    mode: LoopDragMode::Body,
                    origin: range,
                    start_pointer: pos,
                });
            }
            body_response = Some(response);
        }

        let mut start_response = None;
        let mut end_response = None;
        for (mode, edge, x) in [
            (LoopDragMode::Start, LoopEdge::Start, loop_rect.left()),
            (LoopDragMode::End, LoopEdge::End, loop_rect.right()),
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
                let grip = Rect::from_center_size(
                    egui::pos2(x, loop_rect.center().y),
                    egui::vec2(2.0, loop_rect.height().max(6.0)),
                );
                ui.painter()
                    .rect_filled(grip, egui::CornerRadius::same(1), theme::TEXT_MUTED);
            }
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                drag = Some(LoopDrag {
                    mode,
                    origin: range,
                    start_pointer: pos,
                });
            }
            match mode {
                LoopDragMode::Start => start_response = Some(response),
                LoopDragMode::End => end_response = Some(response),
                LoopDragMode::Body => {}
            }
        }

        if let Some(active) = drag {
            ui.data_mut(|data| data.insert_temp(drag_key, active));
        } else {
            ui.data_mut(|data| data.remove::<LoopDrag>(drag_key));
        }

        let mut preview = None;
        let mut committed = None;
        if let Some(active) = drag
            && let Some(pos) = ui.input(|input| input.pointer.interact_pos())
        {
            let bypass = ui.input(|input| self.grid.bypassed(input.modifiers));
            preview = self.loop_drag_candidate(active, pos, content_left, bypass);
        }
        let stopped = body_response.is_some_and(|response| response.drag_stopped())
            || start_response.is_some_and(|response| response.drag_stopped())
            || end_response.is_some_and(|response| response.drag_stopped());
        if stopped && drag.is_some() {
            committed = preview;
            preview = None;
            ui.data_mut(|data| data.remove::<LoopDrag>(drag_key));
        }
        (preview, committed)
    }

    fn loop_drag_candidate(
        &self,
        drag: LoopDrag,
        pointer: egui::Pos2,
        content_left: f32,
        bypass: bool,
    ) -> Option<RangeMus> {
        match drag.mode {
            LoopDragMode::Body => {
                let delta = f64::from(pointer.x - drag.start_pointer.x)
                    / f64::from(self.grid.pixels_per_beat);
                let shifted = micro_time(drag.origin.start.as_f64() + delta);
                let start = self.grid.snap(shifted, bypass).max(TimeMus::zero());
                RangeMus::new(start, start + drag.origin.length()).ok()
            }
            LoopDragMode::Start | LoopDragMode::End => {
                let raw = self.grid.beat_for_x(pointer.x, content_left);
                let beat = self.grid.snap(raw, bypass);
                let edge = if drag.mode == LoopDragMode::Start {
                    LoopEdge::Start
                } else {
                    LoopEdge::End
                };
                loop_edge_range(edge, drag.origin, beat)
            }
        }
    }
}

fn micro_time(value: f64) -> TimeMus {
    TimeMus::new((value * 1_000_000.0).round() as i64, 1_000_000)
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

fn loop_rect(
    rect: Rect,
    content_left: f32,
    grid: TimelineGrid,
    range: RangeMus,
    bottom_inset: f32,
) -> Rect {
    let top = rect.top() + 14.0;
    let bottom = rect.bottom() - bottom_inset.max(0.0);
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
