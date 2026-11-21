use egui::{Id, Rect, Vec2};
use notwig_core::TimeMus;

use crate::widgets::SnapSettings;

use super::geometry::{ROW_HEIGHT, clamp_pitch_scroll};
use super::state::MidiEditorState;

pub(super) const SCROLLBAR_H: f32 = 18.0;

/// Updates the viewport in place from this frame's zoom and pan input.
///
/// Callers must derive pixels-per-beat and the snap step after this returns.
/// Deriving them first mixes the previous scale with the new viewport start,
/// which makes zooming visibly jitter.
pub(super) fn midi_editor_navigation(
    state: &mut MidiEditorState,
    ui: &egui::Ui,
    content: Rect,
    note_area: Rect,
) {
    if !ui.rect_contains_pointer(content) {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let zoom_delta = ui.input(egui::InputState::zoom_delta);
    if (zoom_delta - 1.0).abs() > 1e-4 {
        state
            .viewport
            .zoom_at(pointer.x, content.left(), zoom_delta);
    }
    let middle = ui.input(|input| input.pointer.middle_down());
    if middle {
        let delta = ui.input(|input| input.pointer.delta());
        if delta.x.abs() > 0.0 {
            state
                .viewport
                .pan_beats(-f64::from(delta.x) / f64::from(state.viewport.pixels_per_beat));
        }
        if delta.y.abs() > 0.0 {
            state.pitch_scroll -= delta.y / ROW_HEIGHT;
            state.pitch_scroll = clamp_pitch_scroll(state.pitch_scroll, note_area.height());
        }
    }
    let scroll = ui.input(|input| input.smooth_scroll_delta);
    let shift = ui.input(|input| input.modifiers.shift);
    if !middle {
        if shift || scroll.x.abs() > scroll.y.abs() {
            if scroll.x.abs() > 0.01 {
                state
                    .viewport
                    .pan_beats(-f64::from(scroll.x) / f64::from(state.viewport.pixels_per_beat));
            }
        } else if scroll.y.abs() > 0.01 {
            state.pitch_scroll -= scroll.y / ROW_HEIGHT;
            state.pitch_scroll = clamp_pitch_scroll(state.pitch_scroll, note_area.height());
        }
    }
}

pub(super) fn midi_editor_scrollbar(
    ui: &mut egui::Ui,
    state: &mut MidiEditorState,
    snap: &mut SnapSettings,
    rect: Rect,
    content: Rect,
    content_end: TimeMus,
    ppb: f32,
    beats_per_bar: u8,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - SCROLLBAR_H + 2.0),
        Vec2::new(rect.width(), SCROLLBAR_H - 2.0),
    );
    let scroll_right = bar.right() - crate::timeline::CONTROLS_WIDTH - 8.0;
    let track = Rect::from_min_max(
        egui::pos2(content.left() + 4.0, bar.top() + 4.0),
        egui::pos2(scroll_right - 4.0, bar.bottom() - 4.0),
    );
    let visible = f64::from((content.width() / ppb.max(0.001)).max(1.0));
    let scroll_rect = Rect::from_min_max(bar.min, egui::pos2(scroll_right, bar.bottom()));
    if let Some(start) = crate::timeline::horizontal_scrollbar(
        ui,
        scroll_rect,
        track,
        content_end.as_f64(),
        visible,
        state.viewport.start,
        Id::new(("midi-editor-hscroll", state.clip_id)),
    ) {
        state.viewport.start = start;
    }
    let controls = Rect::from_min_max(
        egui::pos2(
            bar.right() - crate::timeline::CONTROLS_WIDTH - 4.0,
            bar.top(),
        ),
        egui::pos2(bar.right() - 4.0, bar.bottom()),
    );
    crate::timeline::snap_controls(ui, controls, "midi-editor", snap, ppb, beats_per_bar);
}
