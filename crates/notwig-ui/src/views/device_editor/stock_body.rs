use egui::{Color32, CornerRadius, RichText};
use notwig_core::dsp::DeviceSpec;
use notwig_core::{DeviceInstance, Norm01, ParamValue};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_param;

use super::knob::knob;

impl NotwigUi {
    pub(super) fn tool_body(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
        width: f32,
    ) {
        let button_width = ((width - 32.0) / 3.0).max(42.0);
        ui.horizontal(|ui| {
            for (id, label) in [
                (notwig_core::dsp::stock::p_polarity_l(), "L−"),
                (notwig_core::dsp::stock::p_swap_lr(), "Swap L/R"),
                (notwig_core::dsp::stock::p_polarity_r(), "R−"),
            ] {
                let current = device
                    .param_values
                    .get(&id)
                    .cloned()
                    .unwrap_or(ParamValue::Bool(false));
                let active = matches!(current, ParamValue::Bool(true));
                let button = egui::Button::new(RichText::new(label).small().color(if active {
                    Color32::BLACK
                } else {
                    theme::TEXT_MUTED
                }))
                .fill(if active {
                    theme::ACCENT
                } else {
                    theme::PANEL_LIGHT
                })
                .corner_radius(CornerRadius::same(4));
                if ui.add_sized([button_width, 24.0], button).clicked() {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: id,
                        value: ParamValue::Bool(!active),
                    });
                }
            }
        });
        ui.add_space(4.0);
        let columns = [
            (notwig_core::dsp::stock::p_input_gain(), "Volume"),
            (notwig_core::dsp::stock::p_output_gain(), "Gain"),
            (notwig_core::dsp::stock::p_pan(), "Pan"),
            (notwig_core::dsp::stock::p_width(), "Width"),
        ];
        for row in columns.chunks(2) {
            ui.horizontal(|ui| {
                let gap = ((width - 16.0 - 156.0) / 3.0).max(0.0);
                ui.add_space(gap);
                for (id, label) in row {
                    let Some(param) = spec.params.iter().find(|param| param.id == *id) else {
                        continue;
                    };
                    let current = device
                        .param_values
                        .get(&param.id)
                        .cloned()
                        .unwrap_or_else(|| param.default.clone());
                    let mut norm = param.norm(&current).0;
                    let value = match *id {
                        id if id == notwig_core::dsp::stock::p_pan() => {
                            format!("{:.0} %", param.physical(&current) * 100.0)
                        }
                        id if id == notwig_core::dsp::stock::p_width() => {
                            format!("{:.0} %", param.physical(&current) * 100.0)
                        }
                        _ => format_param(param.unit, param.physical(&current)),
                    };
                    let bipolar = param.range.min < 0.0 && param.range.max > 0.0;
                    let default_norm = param.norm(&param.default).0;
                    if knob(
                        ui,
                        ui.make_persistent_id(("knob", device.id, param.id)),
                        &mut norm,
                        default_norm,
                        &value,
                        label,
                        bipolar,
                    ) {
                        self.emit(UiIntent::SetParam {
                            device_id: device.id,
                            param_id: param.id,
                            value: param.value_from_norm(Norm01::clamped(norm)),
                        });
                    }
                    ui.add_space(gap);
                }
            });
        }
    }

    pub(super) fn chain_body(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        spec: &DeviceSpec,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            for (id, label) in [
                (notwig_core::dsp::stock::p_input_gain(), "Wet Gain"),
                (notwig_core::dsp::stock::p_dry_wet(), "Mix"),
            ] {
                let Some(param) = spec.params.iter().find(|param| param.id == id) else {
                    continue;
                };
                let current = device
                    .param_values
                    .get(&param.id)
                    .cloned()
                    .unwrap_or_else(|| param.default.clone());
                let mut norm = param.norm(&current).0;
                let default_norm = param.norm(&param.default).0;
                if knob(
                    ui,
                    ui.make_persistent_id(("chain-knob", device.id, param.id)),
                    &mut norm,
                    default_norm,
                    &format_param(param.unit, param.physical(&current)),
                    label,
                    param.range.min < 0.0 && param.range.max > 0.0,
                ) {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: param.id,
                        value: param.value_from_norm(Norm01::clamped(norm)),
                    });
                }
                ui.add_space(2.0);
            }
        });
    }
}
