use egui::{Color32, RichText, Vec2};
use notwig_core::{
    DeviceId, DeviceInstance, DeviceKind, Norm01, ParamId, ParamValue, ProjectDoc, TrackId,
};

use notwig_dsp::ParamSpec;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::{NotwigUi, selected_device};
use crate::theme;
use crate::widgets::format_param;

impl NotwigUi {
    pub(crate) fn inspector_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        egui::SidePanel::right("notwig_inspector")
            .resizable(true)
            .default_width(310.0)
            .min_width(230.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Device");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            self.show_inspector = false;
                        }
                    });
                });
                ui.separator();
                self.device_content(ui, doc, runtime, false);
            });
    }

    pub(crate) fn device_central(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc) {
        ui.add_space(8.0);
        ui.heading("Device");
        ui.separator();
        self.device_content(ui, doc, &RuntimeView::default(), true);
    }

    fn device_content(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        _runtime: &RuntimeView,
        wide: bool,
    ) {
        let Some(track_id) = self.selected_track(doc) else {
            ui.label(
                RichText::new("Select a track to view its device chain.").color(theme::TEXT_MUTED),
            );
            return;
        };
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&track.name).strong().size(15.0));
            ui.label(
                RichText::new(track.kind.name().to_uppercase())
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        });
        ui.add_space(4.0);
        if let Some(chain) = doc.chains.get(&track.main_chain) {
            let device_ids = chain.devices.clone();
            ui.label(
                RichText::new(format!("{} device(s)", device_ids.len()))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(2.0);
            for device_id in &device_ids {
                self.device_row(ui, doc, *device_id, track_id);
            }
            if device_ids.is_empty() {
                ui.label(
                    RichText::new("No devices. Insert from the browser or use the buttons below.")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            }
            ui.separator();
            if let Some(device_id) = selected_device(doc, self)
                .filter(|id| device_ids.contains(id))
                .or_else(|| device_ids.first().copied())
            {
                self.param_editor(ui, doc, device_id, wide);
            } else {
                ui.label(
                    RichText::new("Select or insert a device to edit parameters.")
                        .color(theme::TEXT_MUTED),
                );
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("+ Tool").clicked() {
                    self.emit(UiIntent::InsertStockDevice {
                        track_id,
                        chain_id: track.main_chain,
                        type_id: notwig_dsp::stock::TOOL_TYPE_ID.into(),
                        kind: DeviceKind::AudioFx,
                    });
                }
                if ui.button("+ EQ").clicked() {
                    self.emit(UiIntent::InsertStockDevice {
                        track_id,
                        chain_id: track.main_chain,
                        type_id: notwig_dsp::stock::EQ_TYPE_ID.into(),
                        kind: DeviceKind::AudioFx,
                    });
                }
                if ui.button("+ Chain").clicked() {
                    self.emit(UiIntent::InsertStockDevice {
                        track_id,
                        chain_id: track.main_chain,
                        type_id: notwig_dsp::stock::CHAIN_TYPE_ID.into(),
                        kind: DeviceKind::Container,
                    });
                }
            });
        }
    }

    fn device_row(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        device_id: DeviceId,
        track_id: TrackId,
    ) {
        let Some(device) = doc.devices.get(&device_id) else {
            return;
        };
        let selected = self.state.selection.devices.contains(&device_id);
        let frame = egui::Frame::new()
            .fill(if selected {
                theme::ACCENT_DIM
            } else {
                theme::PANEL
            })
            .inner_margin(egui::Margin::symmetric(6, 4))
            .corner_radius(egui::CornerRadius::same(4));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = device
                    .name
                    .clone()
                    .unwrap_or_else(|| device.type_id.clone());
                if ui
                    .add(
                        egui::Label::new(RichText::new(label).strong().color(if selected {
                            Color32::WHITE
                        } else {
                            theme::TEXT_MUTED
                        }))
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    self.state.selection.devices.clear();
                    self.state.selection.devices.insert(device_id);
                    self.state.selection.select_only_track(track_id);
                    self.emit(UiIntent::SelectDevice(device_id));
                }
                ui.label(
                    RichText::new(device_kind_label(device.kind))
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").clicked() {
                        self.emit(UiIntent::RemoveDevice(device_id));
                    }
                    let mut bypass = device.bypass;
                    if ui.checkbox(&mut bypass, "Bypass").changed() {
                        self.emit(UiIntent::SetDeviceBypass { device_id, bypass });
                    }
                });
            });
        });
        ui.add_space(2.0);
    }

    fn param_editor(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        device_id: DeviceId,
        wide: bool,
    ) {
        let Some(device) = doc.devices.get(&device_id) else {
            return;
        };
        let Some(spec) = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned()
        else {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("No parameter metadata")
                        .strong()
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(
                        "This device is unresolved. Relink or install the plugin, or insert a stock device.",
                    )
                    .small()
                    .color(theme::TEXT_MUTED),
                );
            });
            return;
        };
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{} · parameters", spec.name))
                .strong()
                .color(theme::ACCENT),
        );
        ui.add_space(2.0);
        let columns = if wide { 3 } else { 1 };
        let params = spec.params.clone();
        if columns == 1 {
            for param in &params {
                self.param_control(ui, device, param, 0);
            }
        } else {
            egui::Grid::new(("device_params", device_id))
                .num_columns(3)
                .spacing(Vec2::new(16.0, 4.0))
                .show(ui, |ui| {
                    for (index, param) in params.iter().enumerate() {
                        self.param_control(ui, device, param, index);
                        if (index + 1) % 3 == 0 {
                            ui.end_row();
                        }
                    }
                });
        }
    }

    fn param_control(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        param: &ParamSpec,
        _index: usize,
    ) {
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
                let label = param_text(&param.name, &current, param);
                egui::ComboBox::from_id_salt(("enum", device.id, param.id))
                    .selected_text(label)
                    .width(ui.available_width().min(150.0))
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
                ui.label(RichText::new(&param.name).small().color(theme::TEXT_MUTED));
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

fn device_kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Instrument => "INSTRUMENT",
        DeviceKind::AudioFx => "AUDIO FX",
        DeviceKind::NoteFx => "NOTE FX",
        DeviceKind::Container => "CONTAINER",
    }
}

fn param_text(name: &str, value: &ParamValue, param: &ParamSpec) -> String {
    if param.kind == notwig_dsp::ParamKind::Bool {
        return if matches!(value, ParamValue::Bool(true)) {
            format!("{name}: on")
        } else {
            format!("{name}: off")
        };
    }
    let index = value.as_f64().unwrap_or(0.0).round() as u32;
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
        format!("{name}: {index}")
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

#[allow(dead_code)]
fn _keep(_: ParamId) {}
