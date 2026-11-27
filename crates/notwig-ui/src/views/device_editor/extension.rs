use std::collections::BTreeMap;

use egui::{Color32, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, ParamKind};
use notwig_core::{DeviceInstance, Norm01, ParamValue};
use notwig_ext_api::{
    Color, DeviceDrawContext, DeviceMeterSnapshot, PanelAction, PanelInput, ThemeSnapshot,
};

use super::panel_render::{paint_panel_frame, panel_input, to_cursor};
use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    pub(crate) fn draw_extension_device_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        sample_rate: u32,
        meter: DeviceMeterSnapshot,
        size: Vec2,
    ) -> bool {
        let selected = self.state.selection.devices.contains(&device.id);
        let (target, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if response.clicked() {
            response.request_focus();
        }
        let focused = response.has_focus();
        let panel_input = panel_input(ui, target, size, focused);
        let context = device_context(
            device,
            spec,
            sample_rate,
            selected,
            meter,
            panel_input,
            size,
        );
        let key = device.id.to_string();
        let frame = if let Some(host) = self.device_panel_host.as_mut() {
            host.draw_device_panel(&device.type_id, &context)
        } else {
            None
        };
        let frame = match frame {
            Some(frame) if frame.validate().is_ok() => {
                self.last_extension_frames
                    .insert(key.clone(), frame.clone());
                Some(frame)
            }
            _ => self.last_extension_frames.get(&key).cloned(),
        };
        let Some(frame) = frame else {
            paint_panel_placeholder(ui, target);
            return false;
        };
        paint_panel_frame(self, ui, device, spec, &frame, target);
        if frame.wants_keyboard && focused {
            self.extension_panel_keyboard = true;
        }
        true
    }

    pub(super) fn apply_panel_actions(
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
                    ui.ctx().set_cursor_icon(to_cursor(*cursor));
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
    selected: bool,
    meter: DeviceMeterSnapshot,
    input: PanelInput,
    size: Vec2,
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
        meter,
    }
}

pub(crate) fn theme_snapshot() -> ThemeSnapshot {
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

fn validated_param_value(
    param: &notwig_core::dsp::ParamSpec,
    value: &ParamValue,
) -> Option<ParamValue> {
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

fn to_color32(color: Color32) -> Color {
    let [r, g, b, a] = color.to_array();
    Color::rgba(r, g, b, a)
}

fn paint_panel_placeholder(ui: &egui::Ui, target: egui::Rect) {
    let target = target.intersect(ui.clip_rect());
    ui.painter().with_clip_rect(target).text(
        target.center(),
        egui::Align2::CENTER_CENTER,
        "Extension panel unavailable",
        egui::FontId::proportional(11.0),
        crate::theme::TEXT_MUTED,
    );
}
