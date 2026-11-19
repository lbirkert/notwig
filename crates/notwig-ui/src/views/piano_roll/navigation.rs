use egui::{Id, Rect, Vec2};
use notwig_core::TimeMus;

use super::geometry::{ROW_HEIGHT, clamp_pitch_scroll};
use super::state::PianoRollState;

pub(super) const SCROLLBAR_H: f32 = 14.0;

pub(super) fn piano_roll_navigation(
    state: &mut PianoRollState,
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

pub(super) fn piano_roll_scrollbar(
    ui: &mut egui::Ui,
    state: &mut PianoRollState,
    rect: Rect,
    content: Rect,
    content_end: TimeMus,
    ppb: f32,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - SCROLLBAR_H + 2.0),
        Vec2::new(rect.width(), SCROLLBAR_H - 2.0),
    );
    let track = Rect::from_min_max(
        egui::pos2(content.left() + 4.0, bar.top() + 3.0),
        egui::pos2(bar.right() - 4.0, bar.bottom() - 3.0),
    );
    let visible = f64::from((content.width() / ppb.max(0.001)).max(1.0));
    if let Some(start) = crate::timeline::horizontal_scrollbar(
        ui,
        bar,
        track,
        content_end.as_f64(),
        visible,
        state.viewport.start,
        Id::new(("piano-roll-hscroll", state.clip_id)),
    ) {
        state.viewport.start = start;
    }
}
