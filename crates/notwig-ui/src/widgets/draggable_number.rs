use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::fine_drag::FineDrag;
use crate::theme;

const CONTROL_W: f32 = 72.0;
const CONTROL_H: f32 = 20.0;

/// Draws a compact monospace numeric field that can be dragged, typed into,
/// and reset.
///
/// Interaction model:
/// - Drag horizontally or vertically for a coarse change.
/// - Shift+drag for fine adjustment.
/// - Cmd/Ctrl+click to type an exact value.
/// - Enter or losing focus commits the typed value; Escape cancels.
/// - Double-click resets to `default`.
///
/// Returns the new value when it changed.
pub(crate) fn draggable_number(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: f64,
    min: f64,
    max: f64,
    default: f64,
    speed: f64,
    format: impl Fn(f64) -> String + Copy,
) -> Option<f64> {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(CONTROL_W, CONTROL_H), Sense::click_and_drag());
    let response = response.on_hover_text(
        "Drag to adjust · Shift for fine · Cmd/Ctrl+click to type · Double-click to reset",
    );
    if response.hovered() || response.dragged() {
        let typing = ui.input(|input| input.modifiers.command || input.modifiers.ctrl);
        ui.ctx().set_cursor_icon(if typing {
            egui::CursorIcon::Text
        } else {
            egui::CursorIcon::ResizeHorizontal
        });
    }

    let edit_id = id.with("draggable-number-edit");
    if let Some((mut text, mut focused)) = ui.data(|data| data.get_temp::<(String, bool)>(edit_id))
    {
        let mut commit = None;
        let mut cancel = false;
        egui::Area::new(edit_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + egui::vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                let edit = ui.add_sized(
                    [CONTROL_W, CONTROL_H],
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(CONTROL_W)
                        .font(egui::TextStyle::Monospace),
                );
                if !focused {
                    edit.request_focus();
                    focused = true;
                } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    commit = Some(parse_value(&text, min, max, value));
                } else if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                } else if !edit.has_focus() {
                    commit = Some(parse_value(&text, min, max, value));
                }
            });
        if let Some(value) = commit {
            ui.data_mut(|data| data.remove::<(String, bool)>(edit_id));
            draw_field(ui, rect, value, &response, format);
            return Some(value);
        }
        if cancel {
            ui.data_mut(|data| data.remove::<(String, bool)>(edit_id));
        } else {
            ui.data_mut(|data| data.insert_temp(edit_id, (text, focused)));
        }
        draw_field(ui, rect, value, &response, format);
        return None;
    }

    let modifiers = ui.input(|input| input.modifiers);
    if response.double_clicked() {
        let reset = clamp_value(default, min, max);
        draw_field(ui, rect, reset, &response, format);
        return ((reset - value).abs() > f64::EPSILON).then_some(reset);
    }
    if response.clicked() && (modifiers.command || modifiers.ctrl) {
        ui.data_mut(|data| {
            data.insert_temp(edit_id, (format!("{value}"), false));
        });
        draw_field(ui, rect, value, &response, format);
        return None;
    }

    let drag_id = id.with("draggable-number-drag");
    if response.drag_started() && !(modifiers.command || modifiers.ctrl) {
        if let Some(origin) = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
        {
            ui.data_mut(|data| data.insert_temp(drag_id, (value, origin, FineDrag::default())));
        }
    }
    let mut new_value = value;
    let mut changed = false;
    if response.dragged() {
        if let Some((start_value, start_pos, mut fine)) =
            ui.data(|data| data.get_temp::<(f64, egui::Pos2, FineDrag)>(drag_id))
        {
            let pos = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                .unwrap_or(start_pos);
            let raw = (pos.x - start_pos.x) - (pos.y - start_pos.y);
            let update = fine.update(raw, ui.input(|input| input.modifiers.shift), 0.1);
            new_value = clamp_value(start_value + update.offset * speed, min, max);
            ui.data_mut(|data| data.insert_temp(drag_id, (start_value, start_pos, fine)));
            changed = (new_value - value).abs() > f64::EPSILON;
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<(f64, egui::Pos2, FineDrag)>(drag_id));
    }
    draw_field(ui, rect, new_value, &response, format);
    changed.then_some(new_value)
}

fn draw_field(
    ui: &egui::Ui,
    rect: Rect,
    value: f64,
    response: &egui::Response,
    format: impl Fn(f64) -> String,
) {
    let painter = ui.painter();
    let active = response.hovered() || response.dragged();
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(18, 19, 23));
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(
            1.0_f32,
            if active {
                theme::ACCENT_DIM
            } else {
                theme::GRID
            },
        ),
        StrokeKind::Inside,
    );
    painter.with_clip_rect(rect.shrink(3.0)).text(
        rect.center(),
        Align2::CENTER_CENTER,
        format(value),
        FontId::monospace(10.5),
        if active {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        },
    );
}

fn clamp_value(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

fn dragged_value(start: f64, dx: f32, dy: f32, speed: f64, fine: bool, min: f64, max: f64) -> f64 {
    let delta = f64::from(dx - dy);
    let factor = if fine { speed * 0.1 } else { speed };
    clamp_value(start + delta * factor, min, max)
}

fn parse_value(text: &str, min: f64, max: f64, fallback: f64) -> f64 {
    text.trim()
        .parse::<f64>()
        .map_or(fallback, |value| clamp_value(value, min, max))
}

#[doc(hidden)]
pub mod test_api {
    pub fn dragged_value(
        start: f64,
        dx: f32,
        dy: f32,
        speed: f64,
        fine: bool,
        min: f64,
        max: f64,
    ) -> f64 {
        super::dragged_value(start, dx, dy, speed, fine, min, max)
    }

    pub fn parse_value(text: &str, min: f64, max: f64, fallback: f64) -> f64 {
        super::parse_value(text, min, max, fallback)
    }
}
