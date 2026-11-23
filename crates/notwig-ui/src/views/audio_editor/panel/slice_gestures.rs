use egui::{Id, Rect, Sense};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipEntity, TempoMap, TimeMus};

use crate::shell::NotwigUi;
use crate::timeline::TimelineGrid;
use crate::timeline::interaction::drag_start_pos;
use crate::widgets::SnapSettings;

use super::super::geometry::finish_slice_drag;
use super::super::state::{ClipEditorState, SliceDrag, SliceDragMode};
use super::range_selection::clear_empty_time_selection;
use super::slice_paint::SlicePaint;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_slice(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    slices: &[AudioSlice],
    map: &TempoMap,
    clip: &ClipEntity,
    slice: &AudioSlice,
    paint: &SlicePaint,
    ppb: f32,
    grid: TimeMus,
    snap: SnapSettings,
    timeline_grid: TimelineGrid,
    content_left: f32,
    bypass: bool,
) {
    let slice_rect = paint.slice_rect;
    let title_rect = paint.title_rect;
    let body_rect = paint.body_rect;
    let body_resp = ui.interact(
        body_rect,
        Id::new(("slice-body", slice.id)),
        Sense::click_and_drag(),
    );
    if body_resp.clicked() {
        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        state.select_node(slice.id, additive);
    }
    if body_resp.drag_started() {
        if let Some(pos) = drag_start_pos(ui, &body_resp) {
            let bypass = snap.bypassed(ui.input(|input| input.modifiers));
            let beat = timeline_grid
                .snap(timeline_grid.beat_for_x(pos.x, content_left), bypass)
                .max(TimeMus::zero());
            if let Ok(range) = notwig_core::RangeMus::new(beat, beat) {
                state.select_bar(range);
            }
            state.range_start = Some(beat);
        }
    }
    if body_resp.dragged()
        && let (Some(origin), Some(pos)) = (state.range_start, body_resp.interact_pointer_pos())
    {
        let bypass = snap.bypassed(ui.input(|input| input.modifiers));
        let beat = timeline_grid
            .snap(timeline_grid.beat_for_x(pos.x, content_left), bypass)
            .max(TimeMus::zero());
        if let Some(range) = crate::timeline::interaction::selection_range(origin, beat) {
            state.select_bar(range);
        }
    }
    if body_resp.drag_stopped() && state.range_start.take().is_some() {
        clear_empty_time_selection(state);
    }

    let title_resp = ui.interact(
        title_rect,
        Id::new(("slice-title", slice.id)),
        Sense::click_and_drag(),
    );
    if title_resp.hovered() || title_resp.dragged() {
        let cursor = if title_resp.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        };
        ui.ctx().set_cursor_icon(cursor);
    }
    if title_resp.clicked() {
        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        state.select_node(slice.id, additive);
    }
    if title_resp.drag_started() {
        if let Some(pos) = drag_start_pos(ui, &title_resp) {
            let shift = ui.input(|input| input.modifiers.shift);
            if shift {
                let beat = timeline_grid
                    .snap(timeline_grid.beat_for_x(pos.x, content_left), true)
                    .max(TimeMus::zero());
                if let Ok(range) = notwig_core::RangeMus::new(beat, beat) {
                    state.select_bar(range);
                }
                state.range_start = Some(beat);
            } else {
                if !state.selected.contains(&slice.id) {
                    state.select_node(slice.id, false);
                }
                state.drag = Some(SliceDrag {
                    slice_id: slice.id,
                    mode: SliceDragMode::Move,
                    origin_pointer: pos,
                    origin_start: slice.start,
                    origin_source_start: slice.source_start_sample,
                    origin_source_end: slice.source_end_sample,
                    origin_silent_prefix: slice.silent_prefix_samples,
                });
            }
        }
    }
    if title_resp.dragged()
        && let (Some(origin), Some(pos)) = (state.range_start, title_resp.interact_pointer_pos())
    {
        let beat = timeline_grid
            .snap(timeline_grid.beat_for_x(pos.x, content_left), true)
            .max(TimeMus::zero());
        if let Some(range) = crate::timeline::interaction::selection_range(origin, beat) {
            state.select_bar(range);
        }
    }
    if title_resp.drag_stopped() {
        if state.range_start.take().is_some() {
            clear_empty_time_selection(state);
        } else if let Some(drag) = state.drag.take() {
            finish_slice_drag(
                notwig,
                state,
                audio,
                slices.to_vec(),
                drag,
                title_resp.interact_pointer_pos(),
                map,
                bypass,
                clip.range,
                ppb,
                grid,
            );
        }
    }

    for (mode, edge, label) in [
        (SliceDragMode::TrimStart, slice_rect.left(), "start"),
        (SliceDragMode::TrimEnd, slice_rect.right(), "end"),
    ] {
        let handle = Rect::from_min_max(
            egui::pos2(edge - 3.5, title_rect.top()),
            egui::pos2(edge + 3.5, title_rect.bottom()),
        );
        let trim = ui.interact(
            handle,
            Id::new(("slice-trim", slice.id, label)),
            Sense::drag(),
        );
        if trim.hovered() || trim.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            painter.rect_filled(handle, egui::CornerRadius::same(2), egui::Color32::WHITE);
        }
        if trim.drag_started()
            && let Some(pos) = drag_start_pos(ui, &trim)
        {
            state.select_node(slice.id, false);
            state.drag = Some(SliceDrag {
                slice_id: slice.id,
                mode,
                origin_pointer: pos,
                origin_start: slice.start,
                origin_source_start: slice.source_start_sample,
                origin_source_end: slice.source_end_sample,
                origin_silent_prefix: slice.silent_prefix_samples,
            });
        }
        if trim.drag_stopped()
            && let Some(drag) = state.drag.take()
        {
            finish_slice_drag(
                notwig,
                state,
                audio,
                slices.to_vec(),
                drag,
                trim.interact_pointer_pos(),
                map,
                bypass,
                clip.range,
                ppb,
                grid,
            );
        }
    }
}
