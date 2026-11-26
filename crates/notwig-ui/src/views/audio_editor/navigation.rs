use egui::{Id, Rect, Vec2};
use notwig_core::TimeMus;

use crate::widgets::SnapSettings;

use super::SCROLLBAR_H;
use super::state::ClipEditorState;

pub(super) fn clip_editor_navigation(state: &mut ClipEditorState, ui: &egui::Ui, content: Rect) {
    crate::timeline::interaction::timeline_navigation_unbounded(
        ui,
        content,
        content.left(),
        &mut state.viewport,
    );
}

pub(super) fn clip_editor_scrollbar(
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    snap: &mut SnapSettings,
    rect: Rect,
    content: Rect,
    content_end: TimeMus,
    ppb: f32,
    bar_length: TimeMus,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom()),
        Vec2::new(rect.width(), SCROLLBAR_H),
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
        Id::new(("clip-editor-hscroll", state.clip_id)),
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
    crate::timeline::snap_controls(ui, controls, "clip-editor", snap, ppb, bar_length);
}
