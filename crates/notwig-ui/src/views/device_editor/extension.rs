use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};
use notwig_core::{DeviceInstance, Norm01, ParamValue};
use notwig_dsp::{DeviceSpec, ParamKind};
use notwig_ext_api::{
    Color, DeviceDrawContext, PanelAction, PanelCommand, PanelDraw, ThemeSnapshot, UiInput,
};

use super::drawer::{EguiDrawer, to_egui_cursor, ui_input};
use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    pub(crate) fn draw_extension_device_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        sample_rate: u32,
        size: Vec2,
    ) -> bool {
        let selected = self.state.selection.devices.contains(&device.id);
        let (target, _) = ui.allocate_exact_size(size, Sense::hover());
        let input = ui_input(ui, target);
        let context = device_context(device, spec, sample_rate, size, selected, input);
        let panel = {
            let Some(host) = self.device_panel_host.as_mut() else {
                return false;
            };
            host.draw_device_panel(&device.type_id, &context)
        };
        let Some(panel) = panel else {
            return false;
        };
        if panel.validate().is_err() {
            return false;
        }
        self.paint_panel(ui, device, spec, &panel, target);
        true
    }

    fn paint_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        panel: &PanelDraw,
        target: Rect,
    ) {
        let mut drawer = EguiDrawer::new(ui.painter().clone(), target);
        for command in &panel.commands {
            match command {
                PanelCommand::Draw(command) => drawer.paint_command(command),
                PanelCommand::Knob {
                    rect,
                    param_id,
                    label,
                    bipolar,
                } => {
                    let Some(param) = spec.params.iter().find(|param| param.id == *param_id) else {
                        continue;
                    };
                    let rect = to_rect(*rect, target.min);
                    let value = device
                        .param_values
                        .get(param_id)
                        .cloned()
                        .unwrap_or_else(|| param.default.clone());
                    let mut norm = param.norm(&value).0;
                    let default_norm = param.norm(&param.default).0;
                    let changed = crate::widgets::knob(
                        ui,
                        ui.make_persistent_id(("extension-knob", device.id, *param_id)),
                        rect,
                        norm,
                        default_norm,
                        *bipolar,
                        &param.name,
                    );
                    if let Some(changed) = changed {
                        norm = changed;
                        self.emit(crate::intent::UiIntent::SetParam {
                            device_id: device.id,
                            param_id: *param_id,
                            value: param.value_from_norm(Norm01::clamped(norm)),
                        });
                    }
                    ui.painter().text(
                        Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom()),
                            egui::pos2(rect.right(), rect.bottom() + 14.0),
                        )
                        .center(),
                        Align2::CENTER_TOP,
                        label,
                        FontId::proportional(10.0),
                        theme::TEXT_MUTED,
                    );
                }
                PanelCommand::Toggle {
                    rect,
                    param_id,
                    label,
                } => {
                    let rect = to_rect(*rect, target.min);
                    let value = device.param_values.get(param_id);
                    let active = matches!(value, Some(ParamValue::Bool(true)));
                    let response = ui.interact(
                        rect,
                        ui.make_persistent_id(("extension-toggle", device.id, *param_id)),
                        Sense::click(),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        egui::CornerRadius::same(4),
                        if active {
                            theme::ACCENT
                        } else {
                            theme::PANEL_LIGHT
                        },
                    );
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(10.0),
                        if active {
                            Color32::BLACK
                        } else {
                            theme::TEXT_MUTED
                        },
                    );
                    if response.clicked() {
                        self.emit(crate::intent::UiIntent::SetParam {
                            device_id: device.id,
                            param_id: *param_id,
                            value: ParamValue::Bool(!active),
                        });
                    }
                }
                PanelCommand::Label { rect, text } => {
                    let rect = to_rect(*rect, target.min);
                    ui.painter().with_clip_rect(target).text(
                        rect.min,
                        Align2::LEFT_TOP,
                        text,
                        FontId::proportional(11.0),
                        theme::TEXT_MUTED,
                    );
                }
                PanelCommand::Section { rect, title } => {
                    let rect = to_rect(*rect, target.min);
                    ui.painter().with_clip_rect(target).text(
                        rect.min,
                        Align2::LEFT_TOP,
                        title.to_uppercase(),
                        FontId::monospace(9.0),
                        theme::TEXT_MUTED,
                    );
                }
            }
        }
        self.apply_panel_actions(ui, device, spec, &panel.actions);
    }

    fn apply_panel_actions(
        &mut self,
        ui: &egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        actions: &[PanelAction],
    ) {
        for action in actions {
            match action {
                PanelAction::SetParamNormalized { param_id, norm } => {
                    let Some(param) = spec.params.iter().find(|param| param.id == *param_id) else {
                        continue;
                    };
                    self.emit(crate::intent::UiIntent::SetParam {
                        device_id: device.id,
                        param_id: *param_id,
                        value: param.value_from_norm(Norm01::clamped(*norm)),
                    });
                }
                PanelAction::SetParamValue { param_id, value } => {
                    let Some(param) = spec.params.iter().find(|param| param.id == *param_id) else {
                        continue;
                    };
                    let Some(value) = validated_param_value(param, value) else {
                        continue;
                    };
                    self.emit(crate::intent::UiIntent::SetParam {
                        device_id: device.id,
                        param_id: *param_id,
                        value,
                    });
                }
                PanelAction::SetCursor { cursor } => {
                    ui.ctx().set_cursor_icon(to_egui_cursor(*cursor));
                }
                PanelAction::RequestRepaint => ui.ctx().request_repaint(),
            }
        }
    }
}

fn device_context(
    device: &DeviceInstance,
    spec: &DeviceSpec,
    sample_rate: u32,
    size: Vec2,
    selected: bool,
    input: UiInput,
) -> DeviceDrawContext {
    DeviceDrawContext {
        device_id: device.id.to_string(),
        type_id: device.type_id.clone(),
        name: device.name.clone().unwrap_or_else(|| spec.name.clone()),
        sample_rate,
        width: size.x,
        height: size.y,
        selected,
        bypassed: device.bypass,
        params: device
            .param_values
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect::<BTreeMap<_, _>>(),
        theme: theme_snapshot(),
        input,
    }
}

fn theme_snapshot() -> ThemeSnapshot {
    ThemeSnapshot {
        accent: to_color32(theme::ACCENT),
        accent_dim: to_color32(theme::ACCENT_DIM),
        playhead: to_color32(theme::PLAYHEAD),
        panel: to_color32(theme::PANEL),
        panel_light: to_color32(theme::PANEL_LIGHT),
        lane: to_color32(theme::LANE),
        grid: to_color32(theme::GRID),
        grid_strong: to_color32(theme::GRID_STRONG),
        text: Color::rgb(235, 237, 242),
        text_muted: to_color32(theme::TEXT_MUTED),
        selection: to_color32(theme::SELECTION),
        clip_audio: to_color32(theme::CLIP_AUDIO),
        clip_midi: to_color32(theme::CLIP_MIDI),
    }
}

fn validated_param_value(param: &notwig_dsp::ParamSpec, value: &ParamValue) -> Option<ParamValue> {
    match param.kind {
        ParamKind::Bool => match value {
            ParamValue::Bool(value) => Some(ParamValue::Bool(*value)),
            _ => None,
        },
        ParamKind::Enum => match value {
            ParamValue::EnumIndex(value) => {
                Some(ParamValue::EnumIndex((*value).min(param.range.max as u32)))
            }
            _ => None,
        },
        ParamKind::Continuous | ParamKind::Discrete => {
            let raw = match value {
                ParamValue::F64(value) => *value,
                ParamValue::I64(value) => *value as f64,
                _ => return None,
            };
            if !raw.is_finite() {
                return None;
            }
            let clamped = param.range.clamp(raw);
            let stepped = if let Some(step) = param.range.step.filter(|step| *step > 0.0) {
                let steps = ((clamped - param.range.min) / step).round();
                (param.range.min + steps * step).clamp(param.range.min, param.range.max)
            } else {
                clamped
            };
            Some(ParamValue::F64(stepped))
        }
    }
}

fn to_pos(point: notwig_ext_api::Point, origin: Pos2) -> Pos2 {
    egui::pos2(origin.x + point.x, origin.y + point.y)
}

fn to_rect(rect: notwig_ext_api::Rect, origin: Pos2) -> Rect {
    Rect::from_min_max(to_pos(rect.min, origin), to_pos(rect.max, origin))
}

fn to_color32(color: Color32) -> Color {
    let [r, g, b, a] = color.to_array();
    Color::rgba(r, g, b, a)
}
