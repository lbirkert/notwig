use egui::{Id, Rect, Sense, Ui};
use notwig_core::{RangeMus, TimeMus};

use crate::theme;
use crate::timeline::TimelineGrid;
use crate::widgets::beats_to_time;

use super::BeatStrip;

const LOOP_EDGE_WIDTH: f32 = 10.0;

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

impl BeatStrip<'_> {
    pub(super) fn pointer_inside_loop(
        &self,
        rect: Rect,
        content_left: f32,
        pos: egui::Pos2,
    ) -> bool {
        self.loop_range.is_some_and(|range| {
            loop_rect(rect, content_left, self.grid, range, self.loop_bottom_inset).contains(pos)
        })
    }

    pub(super) fn loop_interaction(
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
                let shifted = beats_to_time(drag.origin.start.as_f64() + delta);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::widgets::timeline) enum LoopEdge {
    Start,
    End,
}

pub(in crate::widgets::timeline) fn loop_edge_range(
    edge: LoopEdge,
    range: RangeMus,
    beat: TimeMus,
) -> Option<RangeMus> {
    let candidate = match edge {
        LoopEdge::Start => RangeMus::new(beat, range.end).ok()?,
        LoopEdge::End => RangeMus::new(range.start, beat).ok()?,
    };
    (!candidate.is_empty()).then_some(candidate)
}

pub(super) fn loop_rect(
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
