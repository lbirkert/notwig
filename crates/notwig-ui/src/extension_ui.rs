//! Reusable egui editors for extension panels.

use egui::{Color32, Painter, Rect, Stroke, pos2};
use notwig_ext_api::AudioWaveformPreview;

pub use crate::widgets::curve_selection::{
    CurveEditActions, CurveEditResult, apply_curve_edit_actions,
};
pub use crate::widgets::line_editor::{
    LineEditorConfig, LineEditorResponse, LineEditorState, show, show_with_background,
};
pub use crate::widgets::timeline_grid::TimelineGrid;

/// Draws a host-generated waveform preview as a min/max underlay.
pub fn draw_waveform_preview(painter: &Painter, rect: Rect, preview: &AudioWaveformPreview) {
    if preview.columns.is_empty() || rect.width() <= 0.0 {
        return;
    }
    let painter = painter.with_clip_rect(rect);
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let columns = preview.columns.len();
    for (index, column) in preview.columns.iter().enumerate() {
        let x = rect.left() + (index as f32 + 0.5) / columns as f32 * rect.width();
        let top = center - column.max.clamp(-1.0, 1.0) * half;
        let bottom = center - column.min.clamp(-1.0, 1.0) * half;
        if (bottom - top).abs() < 0.5 {
            continue;
        }
        painter.line_segment(
            [pos2(x, top), pos2(x, bottom)],
            Stroke::new(1.0_f32, Color32::BLACK),
        );
    }
}
