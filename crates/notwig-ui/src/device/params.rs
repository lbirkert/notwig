use egui::RichText;
use notwig_core::{DeviceInstance, Norm01, ParamValue};
use notwig_dsp::ParamSpec;

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_param;

impl NotwigUi {
    pub(super) fn param_editor(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let Some(spec) = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned()
        else {
            ui.label(
                RichText::new("No parameter metadata. Relink or install the plugin.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        };
        ui.columns(2, |columns| {
            for (index, param) in spec.params.iter().enumerate() {
                self.param_control(&mut columns[index % 2], device, param);
            }
        });
    }
    fn param_control(&mut self, ui: &mut egui::Ui, device: &DeviceInstance, param: &ParamSpec) {
        let current = device
            .param_values
            .get(&param.id)
            .cloned()
            .unwrap_or_else(|| param.default.clone());
        match param.kind {
            notwig_dsp::ParamKind::Bool => {
                let mut value = matches!(current, ParamValue::Bool(true));
                if ui.checkbox(&mut value, &param.name).changed() {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: param.id,
                        value: ParamValue::Bool(value),
                    });
                }
            }
            notwig_dsp::ParamKind::Enum | notwig_dsp::ParamKind::Discrete => {
                let values = param.range.enum_values.unwrap_or(1).max(1);
                let mut selected = current
                    .as_f64()
                    .map_or(0, |value| value.round().max(0.0) as u32)
                    .min(values.saturating_sub(1));
                let label = enum_text(&param.name, &current);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&param.name).small().color(theme::TEXT_MUTED));
                    egui::ComboBox::from_id_salt(("enum", device.id, param.id))
                        .selected_text(label)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for index in 0..values {
                                if ui
                                    .selectable_value(
                                        &mut selected,
                                        index,
                                        enum_label(&param.name, index),
                                    )
                                    .clicked()
                                {
                                    self.emit(UiIntent::SetParam {
                                        device_id: device.id,
                                        param_id: param.id,
                                        value: ParamValue::EnumIndex(index),
                                    });
                                }
                            }
                        });
                });
            }
            notwig_dsp::ParamKind::Continuous => {
                let mut norm = param.norm(&current).0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&param.name).small().color(theme::TEXT_MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let physical = param.norm_map.to_physical(norm);
                        ui.label(
                            RichText::new(format_param(param.unit, physical))
                                .small()
                                .monospace()
                                .color(theme::ACCENT),
                        );
                    });
                });
                let response = ui.add(
                    egui::Slider::new(&mut norm, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if response.changed() {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: param.id,
                        value: param.value_from_norm(Norm01::clamped(norm)),
                    });
                }
            }
        }
        ui.add_space(2.0);
    }
}

fn enum_text(name: &str, value: &ParamValue) -> String {
    let index = value.as_f64().unwrap_or(0.0).round() as u32;
    if name.to_ascii_lowercase().contains("type") {
        enum_label(name, index)
    } else {
        index.to_string()
    }
}

fn enum_label(name: &str, index: u32) -> String {
    if name.to_ascii_lowercase().contains("type") {
        match index {
            0 => "Bell".into(),
            1 => "Low Shelf".into(),
            2 => "High Shelf".into(),
            3 => "High Pass".into(),
            4 => "Low Pass".into(),
            other => format!("Mode {other}"),
        }
    } else {
        index.to_string()
    }
}
