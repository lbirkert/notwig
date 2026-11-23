use egui::Rect;
use notwig_core::edit::{self, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{ClipEntity, RangeMus, TempoMap, TimeMus};

use crate::shell::NotwigUi;
use crate::timeline::TimelineGrid;
use crate::timeline::interaction::{drag_current_pos, drag_start_pos};
use crate::widgets::SnapSettings;

use super::super::actions::apply_slice_edit;
use super::super::model::{ClipLane, SliceModel, slice_intersects};
use super::super::state::{BarDrag, ClipEditorState};

pub(super) fn clear_empty_time_selection(state: &mut ClipEditorState) {
    if state
        .bar_selection
        .is_some_and(notwig_core::RangeMus::is_empty)
    {
        state.bar_selection = None;
    }
}

pub(super) fn handle_background(
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    bg_resp: &egui::Response,
    timeline_grid: TimelineGrid,
    content_left: f32,
    snap: SnapSettings,
) {
    if bg_resp.clicked() {
        if let Some(pos) = bg_resp.interact_pointer_pos() {
            let bypass = snap.bypassed(ui.input(|input| input.modifiers));
            let beat = timeline_grid
                .snap(timeline_grid.beat_for_x(pos.x, content_left), bypass)
                .max(TimeMus::zero());
            if let Ok(range) = RangeMus::new(beat, beat) {
                state.select_bar(range);
            }
        }
    }
    if bg_resp.drag_started() {
        if let Some(pos) = drag_start_pos(ui, bg_resp) {
            let bypass = snap.bypassed(ui.input(|input| input.modifiers));
            let beat = timeline_grid
                .snap(timeline_grid.beat_for_x(pos.x, content_left), bypass)
                .max(TimeMus::zero());
            if let Ok(range) = RangeMus::new(beat, beat) {
                state.select_bar(range);
            }
            state.range_start = Some(beat);
        }
    }
    if bg_resp.dragged() {
        if let (Some(origin), Some(pos)) = (state.range_start, bg_resp.interact_pointer_pos()) {
            let bypass = snap.bypassed(ui.input(|input| input.modifiers));
            let beat = timeline_grid
                .snap(timeline_grid.beat_for_x(pos.x, content_left), bypass)
                .max(TimeMus::zero());
            if let Some(range) = crate::timeline::interaction::selection_range(origin, beat) {
                state.select_bar(range);
            }
        }
    }
    if bg_resp.drag_stopped() {
        state.range_start = None;
        clear_empty_time_selection(state);
    }
}

pub(super) fn preview_bar_drag(
    state: &mut ClipEditorState,
    ui: &egui::Ui,
    grid: TimeMus,
    bypass: bool,
    ppb: f32,
) -> TimeMus {
    let bar_preview_shift = state.bar_drag.as_ref().map_or(TimeMus::zero(), |drag| {
        let hover = ui
            .input(|input| input.pointer.hover_pos())
            .unwrap_or(drag.start_pointer);
        let delta = crate::timeline::interaction::pointer_delta(drag.start_pointer, hover, ppb);
        let shift =
            crate::timeline::interaction::snapped_shift(drag.origin.start, delta, grid, bypass);
        shift.max(TimeMus::zero() - drag.origin.start)
    });

    if let Some(drag) = state.bar_drag.as_ref() {
        let range = RangeMus::new(
            drag.origin.start + bar_preview_shift,
            drag.origin.end + bar_preview_shift,
        );
        state.bar_selection = range.ok();
    }

    bar_preview_shift
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_bar_drag(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    map: &TempoMap,
    slices: &[AudioSlice],
    timeline_grid: TimelineGrid,
    content_left: f32,
    lanes: Rect,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
) {
    if let Some(range) = state.bar_selection.filter(|range| !range.is_empty()) {
        let x1 = timeline_grid.x_for_beat(range.start, content_left);
        let x2 = timeline_grid.x_for_beat(range.end, content_left);
        let rect = Rect::from_min_max(
            egui::pos2(x1, lanes.top()),
            egui::pos2(x2.max(x1 + 4.0), lanes.bottom()),
        )
        .intersect(lanes);
        let response = ui.interact(
            rect,
            egui::Id::new(("clip-editor-bar-drag", state.clip_id)),
            egui::Sense::drag(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if response.drag_started() {
            if let Some(pointer) = drag_start_pos(ui, &response) {
                state.bar_drag = Some(BarDrag {
                    start_pointer: pointer,
                    origin: range,
                    slices: {
                        let local = RangeMus::new(
                            range.start - clip.range.start,
                            range.end - clip.range.start,
                        )
                        .unwrap_or(range);
                        slices
                            .iter()
                            .filter(|slice| slice_intersects(map, slice, local))
                            .map(|slice| slice.id)
                            .collect()
                    },
                });
            }
        }
        if response.drag_stopped() {
            if let Some(drag) = state.bar_drag.take() {
                let release = drag_current_pos(ui, &response, drag.start_pointer);
                let delta =
                    crate::timeline::interaction::pointer_delta(drag.start_pointer, release, ppb);
                let delta = crate::timeline::interaction::snapped_shift(
                    drag.origin.start,
                    delta,
                    grid,
                    bypass,
                );
                let local = RangeMus::new(
                    drag.origin.start - clip.range.start,
                    drag.origin.end - clip.range.start,
                )
                .unwrap_or(drag.origin);
                let selection = Selection::time(local, [ClipLane]);
                let model = SliceModel {
                    slices,
                    map: *map,
                    clip_length: clip.range.length(),
                    clip_start: clip.range.start,
                };
                if let Ok(plan) = edit::move_selection(&model, &selection, delta) {
                    apply_slice_edit(notwig, state, audio, clip.range, map, slices, plan);
                }
            }
        }
    }
}

pub(super) fn draw_selection(
    clipped: &egui::Painter,
    state: &ClipEditorState,
    timeline_grid: TimelineGrid,
    content_left: f32,
    lanes: Rect,
    ruler: Rect,
    content: Rect,
) {
    if let Some(range) = state.bar_selection {
        crate::timeline::draw_time_selection_fill(
            clipped,
            timeline_grid,
            content_left,
            lanes,
            range,
        );
        let marker_rect = Rect::from_min_max(
            egui::pos2(content.left(), ruler.top()),
            egui::pos2(content.right(), lanes.bottom()),
        );
        crate::timeline::draw_time_selection_markers(
            clipped,
            timeline_grid,
            content_left,
            marker_rect,
            ruler,
            range,
        );
    }
}
