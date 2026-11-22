use egui::{Id, Rect, Sense};
use notwig_core::edit::Edge;
use notwig_core::{ProjectDoc, RangeMus, TimeMus, TrackId};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::timeline::interaction::{
    drag_current_pos, drag_start_pos, pointer_delta, snapped_start, trim_range,
};

use super::{BarSelectionDrag, BarSelectionEdge};

const EDGE_HANDLE_W: f32 = 8.0;

fn min_selection_length() -> TimeMus {
    TimeMus::new(1, 16)
}

impl NotwigUi {
    pub(super) fn bar_selection_interaction(
        &mut self,
        ui: &mut egui::Ui,
        _doc: &ProjectDoc,
        ppb: f32,
        _start: TimeMus,
        grid: TimeMus,
        content_rect: Rect,
    ) {
        let Some(time) = self.state.selection.time.clone() else {
            return;
        };
        if time.range.is_empty() {
            return;
        }
        let tracks: Vec<TrackId> = time.tracks.iter().copied().collect();
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for track in &tracks {
            if let Some(row) = self.row_rect(*track) {
                min_y = min_y.min(row.top());
                max_y = max_y.max(row.bottom());
            }
        }
        if !min_y.is_finite() || !max_y.is_finite() {
            return;
        }
        let x1 = self
            .state
            .viewport
            .beat_to_x(time.range.start, self.timeline_left);
        let x2 = self
            .state
            .viewport
            .beat_to_x(time.range.end, self.timeline_left);
        let rect = Rect::from_min_max(
            egui::pos2(x1.min(x2), min_y),
            egui::pos2(x1.max(x2).max(x1.min(x2) + 4.0), max_y),
        )
        .intersect(content_rect);
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }

        let body_rect = if rect.width() > EDGE_HANDLE_W * 2.0 {
            Rect::from_min_max(
                egui::pos2(rect.left() + EDGE_HANDLE_W * 0.5, rect.top()),
                egui::pos2(rect.right() - EDGE_HANDLE_W * 0.5, rect.bottom()),
            )
        } else {
            rect
        };
        let start_handle = edge_handle(rect, Edge::Start);
        let end_handle = edge_handle(rect, Edge::End);

        let body_response = ui.interact(body_rect, Id::new("bar-selection-move"), Sense::drag());
        let start_response =
            ui.interact(start_handle, Id::new("bar-selection-start"), Sense::drag());
        let end_response = ui.interact(end_handle, Id::new("bar-selection-end"), Sense::drag());

        self.time_selection_edge_drag(
            ui,
            &start_response,
            BarSelectionEdge::Start,
            time.range,
            &tracks,
            ppb,
            grid,
        );
        self.time_selection_edge_drag(
            ui,
            &end_response,
            BarSelectionEdge::End,
            time.range,
            &tracks,
            ppb,
            grid,
        );

        if body_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if body_response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if body_response.drag_started() {
            if let Some(pointer) = drag_start_pos(ui, &body_response) {
                self.bar_selection_drag = Some(BarSelectionDrag {
                    start_pointer: pointer,
                    origin: time.range,
                    tracks: tracks.clone(),
                    edge: BarSelectionEdge::Body,
                });
            }
        }
        if body_response.dragged() {
            if let Some(drag) = self
                .bar_selection_drag
                .clone()
                .filter(|drag| drag.edge == BarSelectionEdge::Body)
            {
                let current = drag_current_pos(ui, &body_response, drag.start_pointer);
                let bypass = self.state.snap.bypassed(ui.input(|i| i.modifiers));
                let delta = pointer_delta(drag.start_pointer, current, ppb);
                let preview_start = snapped_start(drag.origin.start, delta, grid, bypass);
                let preview_end = preview_start + drag.origin.length();
                if let Ok(range) = RangeMus::new(preview_start, preview_end) {
                    self.state.selection.select_time(range, &drag.tracks);
                }
                self.group_preview.clear();
            }
        }
        if body_response.drag_stopped()
            && self
                .bar_selection_drag
                .as_ref()
                .is_some_and(|drag| drag.edge == BarSelectionEdge::Body)
            && let Some(drag) = self.bar_selection_drag.take()
        {
            let current = drag_current_pos(ui, &body_response, drag.start_pointer);
            let bypass = self.state.snap.bypassed(ui.input(|i| i.modifiers));
            let delta = pointer_delta(drag.start_pointer, current, ppb);
            let requested =
                snapped_start(drag.origin.start, delta, grid, bypass) - drag.origin.start;
            if requested != TimeMus::zero() {
                self.emit(UiIntent::MoveTimeSelection {
                    range: drag.origin,
                    tracks: drag.tracks,
                    delta: requested,
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn time_selection_edge_drag(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        edge: BarSelectionEdge,
        origin: RangeMus,
        tracks: &[TrackId],
        ppb: f32,
        grid: TimeMus,
    ) {
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.drag_started()
            && let Some(pointer) = drag_start_pos(ui, response)
        {
            self.bar_selection_drag = Some(BarSelectionDrag {
                start_pointer: pointer,
                origin,
                tracks: tracks.to_vec(),
                edge,
            });
        }
        if response.dragged()
            && let Some(drag) = self
                .bar_selection_drag
                .clone()
                .filter(|drag| drag.edge == edge)
        {
            let current = drag_current_pos(ui, response, drag.start_pointer);
            let delta = pointer_delta(drag.start_pointer, current, ppb);
            let bypass = self.state.snap.bypassed(ui.input(|i| i.modifiers));
            let core_edge = match edge {
                BarSelectionEdge::Start => Edge::Start,
                BarSelectionEdge::End => Edge::End,
                BarSelectionEdge::Body => return,
            };
            let range = trim_range(
                drag.origin,
                core_edge,
                delta,
                grid,
                bypass,
                min_selection_length(),
            );
            if !range.is_empty() {
                self.state.selection.select_time(range, &drag.tracks);
            }
        }
        if response.drag_stopped()
            && self
                .bar_selection_drag
                .as_ref()
                .is_some_and(|drag| drag.edge == edge)
        {
            self.bar_selection_drag = None;
        }
    }
}

fn edge_handle(rect: Rect, edge: Edge) -> Rect {
    let x = match edge {
        Edge::Start => rect.left(),
        Edge::End => rect.right(),
    };
    Rect::from_min_max(
        egui::pos2(x - EDGE_HANDLE_W * 0.5, rect.top()),
        egui::pos2(x + EDGE_HANDLE_W * 0.5, rect.bottom()),
    )
    .intersect(rect)
}
