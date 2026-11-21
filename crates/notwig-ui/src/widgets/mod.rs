mod draggable_number;
mod fader;
mod knob;
mod snap;

pub(crate) mod brand;
pub(crate) mod drop_target;
pub(crate) mod form;
pub(crate) mod icons;
pub(crate) mod theme;
pub(crate) mod timeline;

pub(crate) use draggable_number::draggable_number;
pub(crate) use fader::{DB_MAX, DB_MIN, volume_fader};
pub(crate) use knob::knob;
pub use snap::{
    SNAP_LEVELS, SnapLevel, SnapSettings, adaptive_step, snap_step, snap_step_label, snap_time,
};

use egui::{Color32, Id, Rect, Sense, Vec2};
use notwig_core::{Db, TimeMus};
use notwig_dsp::Unit;

pub fn snap_label(step: TimeMus, beats_per_bar: u8) -> String {
    let beats = step.as_f64();
    let bar = f64::from(beats_per_bar.max(1));
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
    TimeMus::new((snapped * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn format_beats(time: TimeMus, beats_per_bar: u8) -> String {
    let beats = time.as_f64().max(0.0);
    let bar = (beats / f64::from(beats_per_bar.max(1))).floor() as u64 + 1;
    let beat_in_bar = beats % f64::from(beats_per_bar.max(1));
    let beat = beat_in_bar.floor() as u64 + 1;
    let sixteenth = ((beat_in_bar.fract()) * 4.0).floor() as u64 + 1;
    format!("{bar}.{beat}.{sixteenth}")
}

pub fn draw_stereo_meter(painter: &egui::Painter, rect: Rect, peak_l: f32, peak_r: f32) {
    let gap = 2.0;
    let bar_w = ((rect.width() - gap) * 0.5).max(1.0);
    for (index, peak) in [peak_l, peak_r].into_iter().enumerate() {
        let x = rect.left() + index as f32 * (bar_w + gap);
        let bar = Rect::from_min_max(
            egui::pos2(x, rect.top()),
            egui::pos2(x + bar_w, rect.bottom()),
        );
        painter.rect_filled(
            bar,
            egui::CornerRadius::same(1),
            Color32::from_rgb(13, 14, 17),
        );
        let db = 20.0 * peak.max(1e-6).log10();
        let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let top = bar.bottom() - bar.height() * level;
        let visible = Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bar_w, bar.bottom()));
        let green = Rect::from_min_max(
            egui::pos2(x, bar.top() + bar.height() * 0.25),
            egui::pos2(x + bar_w, bar.bottom()),
        );
        let yellow = Rect::from_min_max(
            egui::pos2(x, bar.top() + bar.height() * 0.08),
            egui::pos2(x + bar_w, bar.top() + bar.height() * 0.25),
        );
        let red = Rect::from_min_max(
            egui::pos2(x, bar.top()),
            egui::pos2(x + bar_w, bar.top() + bar.height() * 0.08),
        );
        for (segment, color) in [
            (green, Color32::from_rgb(91, 196, 88)),
            (yellow, Color32::from_rgb(218, 165, 61)),
            (red, Color32::from_rgb(224, 80, 66)),
        ] {
            let clipped = segment.intersect(visible);
            if clipped.height() > 0.0 {
                painter.rect_filled(clipped, egui::CornerRadius::ZERO, color);
            }
        }
    }
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

pub fn value_drag(
    ui: &mut egui::Ui,
    response: &egui::Response,
    id: Id,
    value: f64,
    default: f64,
    min: f64,
    max: f64,
    speed: f64,
    vertical: bool,
) -> Option<f64> {
    let edit_id = id.with("value-edit");
    if let Some((mut text, focused)) = ui.data(|data| data.get_temp::<(String, bool)>(edit_id)) {
        let mut commit = None;
        let mut keep = true;
        let mut focused = focused;
        egui::Area::new(edit_id)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                let edit = ui.add_sized(
                    [90.0, 22.0],
                    egui::TextEdit::singleline(&mut text).desired_width(90.0),
                );
                if !focused {
                    edit.request_focus();
                    focused = true;
                } else if !edit.has_focus() {
                    commit = Some(
                        text.parse::<f64>()
                            .ok()
                            .map_or(value, |v| v.clamp(min, max)),
                    );
                    keep = false;
                }
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    keep = false;
                }
            });
        if let Some(value) = commit {
            ui.data_mut(|data| data.remove::<(String, bool)>(edit_id));
            return Some(value);
        }
        if keep {
            ui.data_mut(|data| data.insert_temp(edit_id, (text, focused)));
        } else {
            ui.data_mut(|data| data.remove::<(String, bool)>(edit_id));
        }
        return None;
    }

    let modifiers = ui.input(|input| input.modifiers);
    if response.double_clicked() {
        return Some(default.clamp(min, max));
    }
    if response.clicked() && modifiers.command {
        ui.data_mut(|data| {
            data.insert_temp(edit_id, (format!("{value:.3}"), false));
        });
        return None;
    }

    let drag_id = id.with("value-drag");
    if response.drag_started() && !modifiers.command {
        let pos = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
            .unwrap_or(response.rect.center());
        let axis = if vertical { pos.y } else { pos.x };
        ui.data_mut(|data| data.insert_temp(drag_id, (value, axis)));
    }
    if response.dragged() {
        if let Some((start_value, start_axis)) =
            ui.data(|data| data.get_temp::<(f64, f32)>(drag_id))
        {
            let pos = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                .unwrap_or(response.rect.center());
            let axis = if vertical { pos.y } else { pos.x };
            let delta = f64::from(axis - start_axis);
            let direction = if vertical { -1.0 } else { 1.0 };
            let speed = if modifiers.shift { speed * 0.1 } else { speed };
            let new_value = (start_value + direction * delta * speed).clamp(min, max);
            if (new_value - value).abs() > f64::EPSILON {
                return Some(new_value);
            }
        }
    }
    None
}

pub fn format_db(value: Db) -> String {
    if value.0 <= -95.0 {
        "-inf".into()
    } else {
        format!("{:+.1} dB", value.0)
    }
}

pub fn format_param(unit: Unit, value: f64) -> String {
    match unit {
        Unit::Decibels => format!("{value:+.2} dB"),
        Unit::Hertz => {
            if value >= 1000.0 {
                format!("{:.2} kHz", value / 1000.0)
            } else {
                format!("{value:.1} Hz")
            }
        }
        Unit::Percent => format!("{:.0}%", value * 100.0),
        Unit::Seconds => format!("{value:.3} s"),
        Unit::Semitones => format!("{value:+.1} st"),
        Unit::None => format!("{value:.2}"),
    }
}

#[doc(hidden)]
pub mod test_api {
    pub use super::brand::test_api as brand;
    pub use super::draggable_number::test_api as draggable_number;
}
