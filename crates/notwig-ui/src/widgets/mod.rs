#[cfg(feature = "app")]
mod draggable_number;
mod snap;

#[cfg(any(feature = "app", feature = "extension-ui"))]
pub(crate) mod area;
#[cfg(feature = "app")]
pub(crate) mod brand;
#[cfg(feature = "app")]
pub(crate) mod drop_target;
#[cfg(feature = "app")]
pub(crate) mod form;
#[cfg(feature = "app")]
pub(crate) mod icons;
#[cfg(any(feature = "app", feature = "extension-ui"))]
pub(crate) mod line_editor;
#[cfg(feature = "app")]
pub(crate) mod theme;
#[cfg(feature = "app")]
pub(crate) mod timeline;
#[cfg(any(feature = "app", feature = "extension-ui"))]
#[path = "timeline/grid.rs"]
pub(crate) mod timeline_grid;

#[cfg(feature = "app")]
pub(crate) use crate::components::{
    DB_MAX, DB_MIN, draw_stereo_meter, format_db, format_param, knob, value_drag, volume_fader,
};
#[cfg(feature = "app")]
pub(crate) use draggable_number::draggable_number;
pub use snap::{
    SNAP_LEVELS, SnapLevel, SnapSettings, adaptive_step, snap_step, snap_step_label, snap_time,
};

use egui::{Color32, Sense, Vec2};
use notwig_core::TimeMus;

/// Converts a beat position to [`TimeMus`] on the shared micro-beat grid.
///
/// Quantizing every gesture and projection to `1e-6` beats keeps repeated
/// conversions on the same rational grid instead of accumulating denominators.
pub(crate) fn beats_to_time(beats: f64) -> TimeMus {
    TimeMus::new((beats * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn snap_label(step: TimeMus, bar_length: TimeMus) -> String {
    let beats = step.as_f64();
    let bar = bar_length.as_f64().max(f64::EPSILON);
    if beats >= bar {
        let bars = (beats / bar).round() as i64;
        if bars <= 1 {
            "1 BAR SNAP".into()
        } else {
            format!("{bars} BARS SNAP")
        }
    } else if beats >= 1.0 {
        let whole = beats.round() as i64;
        if whole <= 1 {
            "1 BEAT SNAP".into()
        } else {
            format!("{whole} BEATS SNAP")
        }
    } else {
        let denominator = (1.0 / beats).round() as i64;
        format!("1/{denominator} SNAP")
    }
}

pub fn snap_16(time: TimeMus, bypass: bool) -> TimeMus {
    const GRID: f64 = 16.0;
    if bypass {
        return time;
    }
    let beats = time.as_f64();
    let snapped = (beats * GRID).round() / GRID;
    beats_to_time(snapped)
}

pub fn format_beats(time: TimeMus, bar_length: TimeMus, beat_length: TimeMus) -> String {
    let beats = time.as_f64().max(0.0);
    let bar_len = bar_length.as_f64().max(f64::EPSILON);
    let beat_len = beat_length.as_f64().max(f64::EPSILON);
    let bar = (beats / bar_len).floor() as u64 + 1;
    let beat_in_bar = beats % bar_len;
    let beat = (beat_in_bar / beat_len).floor() as u64 + 1;
    let sub_beat = (beat_in_bar % beat_len) / beat_len;
    let sixteenth = (sub_beat * 4.0).floor() as u64 + 1;
    format!("{bar}.{beat}.{sixteenth}")
}

pub fn collapse_handle(
    ui: &mut egui::Ui,
    size: Vec2,
    vertical: bool,
    id_salt: impl std::hash::Hash,
    tooltip: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let response = response.on_hover_text(tooltip);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(if vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
    }
    let painter = ui.painter();
    if response.hovered() || response.dragged() {
        painter.rect_filled(rect, egui::CornerRadius::ZERO, Color32::from_white_alpha(8));
    }
    let color = if response.hovered() || response.dragged() {
        Color32::WHITE
    } else {
        Color32::from_rgb(106, 110, 122)
    };
    let center = rect.center();
    if vertical {
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            egui::Stroke::new(1.0_f32, color),
        );
    } else {
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            egui::Stroke::new(1.0_f32, color),
        );
    }
    let drag_id = ui.make_persistent_id(("collapse-handle-drag", &id_salt));
    if response.drag_started() {
        let origin = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
            .unwrap_or(center);
        ui.data_mut(|data| data.insert_temp(drag_id, if vertical { origin.x } else { origin.y }));
    }
    if response.drag_stopped() {
        let start = ui.data(|data| data.get_temp::<f32>(drag_id));
        let end = ui.input(|input| input.pointer.hover_pos()).map_or(
            if vertical { center.x } else { center.y },
            |pos| {
                if vertical { pos.x } else { pos.y }
            },
        );
        if start.is_some_and(|start| {
            if vertical {
                start - end > 24.0
            } else {
                end - start > 24.0
            }
        }) {
            return true;
        }
    }
    false
}

#[cfg(feature = "app")]
#[doc(hidden)]
pub mod test_api {
    pub use super::brand::test_api as brand;
    pub use super::draggable_number::test_api as draggable_number;
}
