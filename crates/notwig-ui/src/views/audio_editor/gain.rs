//! Simple gain sub-editor for the audio clip editor.

use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Sense, Stroke};
use notwig_core::sequencer::AudioClipData;
use notwig_core::{ClipEntity, Db, TempoMap};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::timeline::TimelineGrid;
use crate::timeline::interaction::drag_start_pos;

use super::model::{emit_slices, source_duration};
use super::state::{ClipEditorState, ClipGainDrag, GainDrag};

/// Lowest gain shown by the gain sub-editor.
pub(super) const MIN_GAIN_DB: f32 = -60.0;
/// Highest gain shown by the gain sub-editor.
pub(super) const MAX_GAIN_DB: f32 = 24.0;
const DB_PER_PIXEL: f32 = 0.35;
pub(super) const LANE_PADDING: f32 = 8.0;

/// Draws the gain surface and applies committed gain edits.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_gain_lane(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    map: &TempoMap,
    timeline_grid: TimelineGrid,
    content_left: f32,
    lanes: Rect,
) {
    let top = lanes.top() + LANE_PADDING;
    let bottom = lanes.bottom() - LANE_PADDING;
    if bottom - top < 16.0 {
        return;
    }
    let painter = painter.with_clip_rect(lanes);
    let zero_y = y_for_gain(0.0, top, bottom);
    painter.line_segment(
        [
            egui::pos2(lanes.left(), zero_y),
            egui::pos2(lanes.right(), zero_y),
        ],
        Stroke::new(1.0_f32, theme::GRID),
    );
    painter.text(
        egui::pos2(lanes.left() + 6.0, zero_y - 2.0),
        Align2::LEFT_BOTTOM,
        "0 dB",
        FontId::proportional(10.0),
        theme::TEXT_MUTED,
    );
    if audio.uses_slices() {
        draw_slice_gains(
            notwig,
            ui,
            &painter,
            state,
            audio,
            clip,
            map,
            timeline_grid,
            content_left,
            lanes,
            top,
            bottom,
        );
    } else {
        draw_clip_gain(notwig, ui, &painter, state, audio, clip, lanes, top, bottom);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_slice_gains(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    map: &TempoMap,
    timeline_grid: TimelineGrid,
    content_left: f32,
    lanes: Rect,
    top: f32,
    bottom: f32,
) {
    let slices = audio.slices.clone();
    if slices.is_empty() {
        painter.text(
            egui::pos2(lanes.center().x, lanes.center().y),
            Align2::CENTER_CENTER,
            "No audio events to adjust.",
            FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        return;
    }
    for slice in &slices {
        let start = clip.range.start + slice.start;
        let x1 = timeline_grid
            .x_for_beat(start, content_left)
            .max(lanes.left());
        let x2 = timeline_grid
            .x_for_beat(start + source_duration(map, slice), content_left)
            .min(lanes.right());
        if x2 - x1 < 1.0 {
            continue;
        }
        let rect = Rect::from_min_max(egui::pos2(x1, top), egui::pos2(x2, bottom));
        let selected = state.selected_contains(slice.id);
        let fill = if selected {
            Color32::from_rgba_unmultiplied(255, 154, 46, 44)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        };
        painter.rect_filled(rect, CornerRadius::same(3), fill);
        let y = y_for_gain(slice.gain_db.0, top, bottom);
        let color = if selected {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        };
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(2.0_f32, color),
        );
        painter.circle_filled(egui::pos2(rect.center().x, y), 4.0, color);
        if rect.width() > 64.0 {
            painter.text(
                egui::pos2(rect.left() + 6.0, top + 4.0),
                Align2::LEFT_TOP,
                format!("{:+.1} dB", slice.gain_db.0),
                FontId::proportional(10.0),
                color,
            );
        }

        let response = ui.interact(
            rect,
            Id::new(("gain-slice", state.clip_id, slice.id)),
            Sense::click_and_drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if response.clicked() {
            let additive = ui.input(|input| input.modifiers.command || input.modifiers.shift);
            state.select_node(slice.id, additive);
        }
        if response.drag_started()
            && let Some(pointer) = drag_start_pos(ui, &response)
        {
            if !state.selected_contains(slice.id) {
                state.select_node(slice.id, false);
            }
            let origin_gains = state
                .selected_slice_ids()
                .into_iter()
                .filter_map(|id| {
                    slices
                        .iter()
                        .find(|slice| slice.id == id)
                        .map(|slice| (id, slice.gain_db))
                })
                .collect();
            state.gain_drag = Some(GainDrag {
                origin_pointer_y: pointer.y,
                origin_gains,
            });
        }
        if response.dragged()
            && let (Some(drag), Some(pointer)) =
                (state.gain_drag.as_ref(), response.interact_pointer_pos())
        {
            apply_gain_delta(drag, pointer.y, audio);
        }
        if response.drag_stopped()
            && let Some(drag) = state.gain_drag.take()
        {
            if let Some(pointer) = response.interact_pointer_pos() {
                apply_gain_delta(&drag, pointer.y, audio);
            }
            let updated = audio.slices.clone();
            emit_slices(notwig, state, audio, updated);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_clip_gain(
    notwig: &mut NotwigUi,
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    clip: &ClipEntity,
    lanes: Rect,
    top: f32,
    bottom: f32,
) {
    let y = y_for_gain(audio.gain_db.0, top, bottom);
    painter.line_segment(
        [egui::pos2(lanes.left(), y), egui::pos2(lanes.right(), y)],
        Stroke::new(2.0_f32, theme::ACCENT),
    );
    painter.circle_filled(egui::pos2(lanes.center().x, y), 5.0, theme::ACCENT);
    painter.text(
        egui::pos2(lanes.left() + 6.0, top + 4.0),
        Align2::LEFT_TOP,
        format!("Clip Gain  {:+.1} dB", audio.gain_db.0),
        FontId::proportional(11.0),
        theme::TEXT_PRIMARY,
    );

    let response = ui.interact(
        Rect::from_min_max(
            egui::pos2(lanes.left(), top),
            egui::pos2(lanes.right(), bottom),
        ),
        Id::new(("clip-gain", state.clip_id)),
        Sense::click_and_drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if response.drag_started()
        && let Some(pointer) = drag_start_pos(ui, &response)
    {
        state.clip_gain_drag = Some(ClipGainDrag {
            origin_pointer_y: pointer.y,
            origin_gain: audio.gain_db,
        });
    }
    if response.dragged()
        && let (Some(drag), Some(pointer)) = (
            state.clip_gain_drag.as_ref(),
            response.interact_pointer_pos(),
        )
    {
        apply_clip_gain_delta(*drag, pointer.y, audio);
    }
    if response.drag_stopped()
        && let Some(drag) = state.clip_gain_drag.take()
    {
        if let Some(pointer) = response.interact_pointer_pos() {
            apply_clip_gain_delta(drag, pointer.y, audio);
        }
        notwig.emit(UiIntent::SetClipGain {
            clip_id: clip.id,
            gain_db: audio.gain_db,
        });
    }
}

fn apply_gain_delta(drag: &GainDrag, pointer_y: f32, audio: &mut AudioClipData) {
    let delta = (drag.origin_pointer_y - pointer_y) * DB_PER_PIXEL;
    for (id, origin) in &drag.origin_gains {
        if let Some(target) = audio.slices.iter_mut().find(|slice| slice.id == *id) {
            target.gain_db = Db((origin.0 + delta).clamp(MIN_GAIN_DB, MAX_GAIN_DB));
        }
    }
}

fn apply_clip_gain_delta(drag: ClipGainDrag, pointer_y: f32, audio: &mut AudioClipData) {
    let delta = (drag.origin_pointer_y - pointer_y) * DB_PER_PIXEL;
    audio.gain_db = Db((drag.origin_gain.0 + delta).clamp(MIN_GAIN_DB, MAX_GAIN_DB));
}

pub(super) fn y_for_gain(db: f32, top: f32, bottom: f32) -> f32 {
    let norm = (db.clamp(MIN_GAIN_DB, MAX_GAIN_DB) - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB);
    bottom - norm * (bottom - top)
}
