use egui::{Id, Rect, Vec2};
use notwig_core::TimeMus;

use super::SCROLLBAR_H;
use super::state::ClipEditorState;

pub(super) fn clip_editor_navigation(state: &mut ClipEditorState, ui: &egui::Ui, content: Rect) {
    crate::timeline::interaction::timeline_navigation(
        ui,
        content,
        content.left(),
        &mut state.viewport,
    );
}

pub(super) fn clip_editor_scrollbar(
    ui: &mut egui::Ui,
    state: &mut ClipEditorState,
    rect: Rect,
    content: Rect,
    content_end: TimeMus,
    ppb: f32,
) {
    let bar = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() + 2.0),
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
        Id::new(("clip-editor-hscroll", state.clip_id)),
    ) {
        state.viewport.start = start;
    }
}
