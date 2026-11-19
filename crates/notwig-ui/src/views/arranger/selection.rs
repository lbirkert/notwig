use egui::{Color32, CornerRadius, Id, Rect, Sense, Vec2};
use notwig_core::{ProjectDoc, RangeMus, TimeMus, TrackId};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::timeline::interaction::{drag_current_pos, drag_start_pos};

use super::BarSelectionDrag;

impl NotwigUi {
    pub(super) fn bar_selection_interaction(
        &mut self,
        ui: &mut egui::Ui,
        _doc: &ProjectDoc,
        ppb: f32,
        start: TimeMus,
        grid: TimeMus,
    ) {
        let Some(time) = self.state.selection.time.clone() else {
            return;
        };
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
        .intersect(ui.max_rect());
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }
        let response = ui.interact(rect, Id::new("bar-selection-move"), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("bar-selection-handle"),
                ))
                .with_clip_rect(rect);
            let handle = Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + 10.0),
                Vec2::new(44.0, 16.0),
            );
            painter.rect_filled(handle, CornerRadius::same(6), Color32::WHITE);
            crate::icons::paint(
                &painter,
                handle,
                crate::icons::Icon::MoveHorizontal,
                Color32::BLACK,
            );
        }
        if response.drag_started() {
            if let Some(pointer) = drag_start_pos(ui, &response) {
                self.bar_selection_drag = Some(BarSelectionDrag {
                    start_pointer: pointer,
                    origin: time.range,
                    tracks,
                });
            }
        }
        if response.dragged() {
            if let Some(drag) = self.bar_selection_drag.clone() {
                let current = drag_current_pos(ui, &response, drag.start_pointer);
                let alt = ui.input(|i| i.modifiers.alt);
                let delta =
                    crate::timeline::interaction::pointer_delta(drag.start_pointer, current, ppb);
                let requested = crate::timeline::interaction::snapped_delta(
                    drag.origin.start,
                    delta,
                    grid,
                    alt,
                );
                let preview_start = drag.origin.start + requested;
                let preview_end = drag.origin.end + requested;
                if let Ok(range) = RangeMus::new(preview_start, preview_end) {
                    self.state.selection.select_time(range, &drag.tracks);
                }
                self.group_preview.clear();
            }
        }
        if response.drag_stopped() {
            if let Some(drag) = self.bar_selection_drag.take() {
                let current = drag_current_pos(ui, &response, drag.start_pointer);
                let alt = ui.input(|i| i.modifiers.alt);
                let delta =
                    crate::timeline::interaction::pointer_delta(drag.start_pointer, current, ppb);
                let requested = crate::timeline::interaction::snapped_delta(
                    drag.origin.start,
                    delta,
                    grid,
                    alt,
                );
                if requested != TimeMus::zero() {
                    self.emit(UiIntent::MoveTimeSelection {
                        range: drag.origin,
                        tracks: drag.tracks,
                        delta: requested,
                    });
                }
            }
        }
        let _ = start;
    }
}
