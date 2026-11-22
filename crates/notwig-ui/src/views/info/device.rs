use egui::{RichText, Vec2};
use notwig_core::{DeviceId, DeviceInstance, ParamValue, ProjectDoc};

use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::views::device_editor::{DeviceRendererKind, device_renderer};
use crate::widgets::format_param;

use super::common::section_header;

impl NotwigUi {
    pub(super) fn device_info(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, device_id: DeviceId) {
        let Some(device) = doc.devices.get(&device_id).cloned() else {
            return;
        };
        let count = self.state.selection.devices.len();
        section_header(ui, if count > 1 { "DEVICES" } else { "DEVICE" });
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} devices selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            let selected: Vec<_> = self.state.selection.devices.iter().copied().collect();
            for id in selected {
                if let Some(other) = doc.devices.get(&id) {
                    let name = other
                        .name
                        .clone()
                        .unwrap_or_else(|| self.device_name(other));
                    if ui.selectable_label(id == device_id, name).clicked() {
                        self.state.selection.devices.set_primary(id);
                    }
                }
            }
            ui.separator();
        }
        ui.horizontal(|ui| {
            if crate::icons::icon_button_sized(
                ui,
                crate::icons::Icon::Power,
                Vec2::new(26.0, 22.0),
                !device.bypass,
                if device.bypass {
                    "Enable device"
                } else {
                    "Bypass device"
                },
            )
            .clicked()
            {
                self.emit(UiIntent::SetDeviceBypass {
                    device_id,
                    bypass: !device.bypass,
                });
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(self.device_name(&device)).strong().size(15.0));
                ui.label(
                    RichText::new(device.type_id.clone())
                        .small()
                        .monospace()
                        .color(theme::TEXT_MUTED),
                );
            });
        });
        ui.label(
            RichText::new(if device.bypass { "Bypassed" } else { "Active" })
                .small()
                .color(if device.bypass {
                    theme::TEXT_MUTED
                } else {
                    theme::ACCENT
                }),
        );
        ui.separator();
        let spec = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned();
        if device_renderer(spec.as_ref()) == DeviceRendererKind::Eq {
            self.eq_info(ui, &device);
        } else {
            self.generic_device_info(ui, &device);
        }
    }

    fn eq_info(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let values = self.device_values(device);
        ui.label(theme::heading(ui.ctx(), "BANDS"));
        for band in 0..4 {
            let enabled = matches!(
                values.get(&notwig_core::dsp::stock::band_param(band, 0)),
                Some(ParamValue::Bool(true))
            );
            let kind = values
                .get(&notwig_core::dsp::stock::band_param(band, 1))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0) as u32;
            let freq = values
                .get(&notwig_core::dsp::stock::band_param(band, 2))
                .and_then(ParamValue::as_f64)
                .unwrap_or(1_000.0);
            let gain = values
                .get(&notwig_core::dsp::stock::band_param(band, 3))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0);
            let q = values
                .get(&notwig_core::dsp::stock::band_param(band, 4))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.707);
            let selected = self.eq_band.min(3) == band;
            egui::Frame::new()
                .fill(if selected {
                    theme::ACCENT_DIM
                } else {
                    theme::PANEL
                })
                .inner_margin(egui::Margin::symmetric(5, 4))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if crate::icons::icon_button_sized(
                            ui,
                            crate::icons::Icon::Power,
                            Vec2::new(20.0, 18.0),
                            enabled,
                            if enabled {
                                "Disable band"
                            } else {
                                "Enable band"
                            },
                        )
                        .clicked()
                        {
                            self.emit(UiIntent::SetParam {
                                device_id: device.id,
                                param_id: notwig_core::dsp::stock::band_param(band, 0),
                                value: ParamValue::Bool(!enabled),
                            });
                        }
                        if ui
                            .selectable_label(
                                selected,
                                format!("{}  {}", band + 1, band_type_label(kind)),
                            )
                            .clicked()
                        {
                            self.eq_band = band;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{freq:.0} Hz · {gain:+.1} dB · {q:.2}"))
                                    .small()
                                    .monospace()
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                });
            ui.add_space(2.0);
        }
        ui.separator();
        egui::CollapsingHeader::new("Spectrum Analyzer")
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.analyzer_pre, "Pre");
                    ui.checkbox(&mut self.analyzer_post, "Post");
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Resolution").small());
                    egui::ComboBox::from_id_salt("analyzer-resolution")
                        .selected_text(["Low", "Medium", "High"][self.analyzer_resolution.min(2)])
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Low", "Medium", "High"].iter().enumerate() {
                                ui.selectable_value(&mut self.analyzer_resolution, index, *label);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Speed").small());
                    egui::ComboBox::from_id_salt("analyzer-speed")
                        .selected_text(["Slow", "Medium", "Fast"][self.analyzer_speed.min(2)])
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Slow", "Medium", "Fast"].iter().enumerate() {
                                ui.selectable_value(&mut self.analyzer_speed, index, *label);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tilt").small());
                    ui.add(
                        egui::DragValue::new(&mut self.analyzer_tilt)
                            .speed(0.1)
                            .suffix(" dB/oct"),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "Range {:.0} … {:.0} dB",
                        self.analyzer_range.0, self.analyzer_range.1
                    ))
                    .small()
                    .color(theme::TEXT_MUTED),
                );
            });
    }

    fn generic_device_info(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let Some(spec) = self.registry.resolve(&device.type_id, device.version) else {
            ui.label(
                RichText::new("No parameter metadata available.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        };
        ui.label(RichText::new(&spec.name).strong().color(theme::TEXT_MUTED));
        for param in spec.params.iter().take(12) {
            let value = device
                .param_values
                .get(&param.id)
                .cloned()
                .unwrap_or_else(|| param.default.clone());
            let physical = param.physical(&value);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&param.name).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_param(param.unit, physical))
                            .small()
                            .monospace()
                            .color(theme::ACCENT),
                    );
                });
            });
        }
    }

    pub(crate) fn device_name(&self, device: &DeviceInstance) -> String {
        device.name.clone().unwrap_or_else(|| {
            self.registry
                .resolve(&device.type_id, device.version)
                .map_or_else(|| device.type_id.clone(), |spec| spec.name.clone())
        })
    }
}

fn band_type_label(index: u32) -> &'static str {
    match index {
        1 => "Low Shelf",
        2 => "High Shelf",
        3 => "High Pass",
        4 => "Low Pass",
        _ => "Bell",
    }
}
