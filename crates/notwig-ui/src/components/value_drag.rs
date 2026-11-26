use egui::{Id, Response};

use crate::fine_drag::FineDrag;

#[derive(Clone)]
struct ValueDrag {
    start_value: f64,
    start_axis: f32,
    fine: FineDrag,
}

/// Handles drag, double-click reset, and command-click numeric entry.
///
/// Returns the new value when it changed this frame.
pub fn value_drag(
    ui: &mut egui::Ui,
    response: &Response,
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
        ui.data_mut(|data| {
            data.insert_temp(
                drag_id,
                ValueDrag {
                    start_value: value,
                    start_axis: axis,
                    fine: FineDrag::default(),
                },
            );
        });
    }
    if response.dragged() {
        if let Some(mut drag) = ui.data(|data| data.get_temp::<ValueDrag>(drag_id)) {
            let pos = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                .unwrap_or(response.rect.center());
            let axis = if vertical { pos.y } else { pos.x };
            let update = drag
                .fine
                .update(axis - drag.start_axis, modifiers.shift, 0.1);
            ui.data_mut(|data| data.insert_temp(drag_id, drag.clone()));
            let direction = if vertical { -1.0 } else { 1.0 };
            let new_value = (drag.start_value + direction * update.offset * speed).clamp(min, max);
            if (new_value - value).abs() > f64::EPSILON {
                return Some(new_value);
            }
        }
    }
    None
}
