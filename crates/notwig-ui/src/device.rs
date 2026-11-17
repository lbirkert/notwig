use std::collections::BTreeMap;

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, Vec2};
use notwig_core::{
    DeviceId, DeviceInstance, DeviceKind, Norm01, ParamId, ParamValue, ProjectDoc, TrackId,
};
use notwig_dsp::ParamSpec;

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_param;

impl NotwigUi {
    pub(crate) fn device_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        egui::TopBottomPanel::bottom("notwig_devices")
            .resizable(true)
            .default_height(285.0)
            .min_height(150.0)
            .max_height(520.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ctx, |ui| {
                let Some(track_id) = self.selected_track(doc) else {
                    ui.label(
                        RichText::new("Select a track to inspect its device chain.")
                            .color(theme::TEXT_MUTED),
                    );
                    return;
                };
                let Some(track) = doc.tracks.get(&track_id) else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("DEVICES")
                            .small()
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.separator();
                    ui.label(RichText::new(&track.name).strong().size(14.0));
                    ui.label(
                        RichText::new(track.kind.name().to_uppercase())
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.push_id(("device-add", "header"), |ui| {
                            self.device_add_menu(ui, doc, runtime, track_id);
                        });
                    });
                });
                ui.separator();
                let device_ids = doc
                    .chains
                    .get(&track.main_chain)
                    .map(|chain| chain.devices.clone())
                    .unwrap_or_default();
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            if device_ids.is_empty() {
                                ui.vertical(|ui| {
                                    ui.add_space(14.0);
                                    ui.label(
                                        RichText::new(
                                            "No devices in this chain. Add Tool, EQ, Chain, or a hosted plugin.",
                                        )
                                        .color(theme::TEXT_MUTED),
                                    );
                                    ui.horizontal(|ui| {
                                        if ui.button("+ Tool").clicked() {
                                            self.insert_stock(track_id, track.main_chain, notwig_dsp::stock::TOOL_TYPE_ID, DeviceKind::AudioFx);
                                        }
                                        if ui.button("+ EQ").clicked() {
                                            self.insert_stock(track_id, track.main_chain, notwig_dsp::stock::EQ_TYPE_ID, DeviceKind::AudioFx);
                                        }
                                        if ui.button("+ Chain").clicked() {
                                            self.insert_stock(track_id, track.main_chain, notwig_dsp::stock::CHAIN_TYPE_ID, DeviceKind::Container);
                                        }
                                    });
                                });
                            } else {
                                for device_id in &device_ids {
                                    self.device_card(ui, doc, *device_id, track_id);
                                }
                                ui.vertical(|ui| {
                                    ui.add_space(8.0);
                                    ui.push_id(("device-add", "trailing"), |ui| {
                                        self.device_add_menu(ui, doc, runtime, track_id);
                                    });
                                });
                            }
                        });
                    });
            });
    }

    fn device_add_menu(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        track_id: TrackId,
    ) {
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        let chain_id = track.main_chain;
        ui.menu_button("+ Device", |ui| {
            if ui.button("Tool").clicked() {
                self.insert_stock(
                    track_id,
                    chain_id,
                    notwig_dsp::stock::TOOL_TYPE_ID,
                    DeviceKind::AudioFx,
                );
                ui.close();
            }
            if ui.button("EQ").clicked() {
                self.insert_stock(
                    track_id,
                    chain_id,
                    notwig_dsp::stock::EQ_TYPE_ID,
                    DeviceKind::AudioFx,
                );
                ui.close();
            }
            if ui.button("Chain").clicked() {
                self.insert_stock(
                    track_id,
                    chain_id,
                    notwig_dsp::stock::CHAIN_TYPE_ID,
                    DeviceKind::Container,
                );
                ui.close();
            }
            if !runtime.plugins.is_empty() {
                ui.separator();
                for plugin in &runtime.plugins {
                    if ui.button(&plugin.name).clicked() {
                        self.emit(UiIntent::InsertPlugin {
                            track_id,
                            chain_id,
                            type_id: plugin.type_id.clone(),
                            kind: plugin.kind,
                        });
                        ui.close();
                    }
                }
            }
        });
    }

    fn insert_stock(
        &mut self,
        track_id: TrackId,
        chain_id: notwig_core::DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
    ) {
        self.emit(UiIntent::InsertStockDevice {
            track_id,
            chain_id,
            type_id: type_id.into(),
            kind,
        });
    }

    fn device_card(
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
        let width = card_width(&device.type_id);
        egui::Frame::new()
            .fill(theme::PANEL)
            .stroke(Stroke::new(
                1.0_f32,
                if selected { theme::ACCENT } else { theme::GRID },
            ))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.push_id(("device-card", device_id), |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, ui.available_height().max(180.0)),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                let mut bypass = device.bypass;
                                let power = if bypass { "OFF" } else { "ON" };
                                let color = if bypass {
                                    theme::TEXT_MUTED
                                } else {
                                    theme::ACCENT
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(power).small().strong())
                                            .fill(theme::PANEL_LIGHT)
                                            .min_size(Vec2::new(34.0, 20.0)),
                                    )
                                    .clicked()
                                {
                                    bypass = !bypass;
                                    self.emit(UiIntent::SetDeviceBypass { device_id, bypass });
                                }
                                let name = device
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| spec_name(self, device));
                                if ui
                                    .add(
                                        egui::Label::new(RichText::new(name).strong().color(
                                            if bypass {
                                                theme::TEXT_MUTED
                                            } else {
                                                Color32::WHITE
                                            },
                                        ))
                                        .sense(Sense::click()),
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
                                        .color(color)
                                        .monospace(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("×").clicked() {
                                            self.emit(UiIntent::RemoveDevice(device_id));
                                        }
                                    },
                                );
                            });
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt("device-params")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
                                        self.eq_curve(ui, device);
                                    }
                                    self.param_editor(ui, device);
                                });
                        },
                    );
                });
            });
        ui.add_space(6.0);
    }

    #[allow(clippy::unused_self)]
    fn eq_curve(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let height = 122.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(200.0), height),
            Sense::hover(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(17, 19, 23));
        let min_f = 20.0_f64;
        let max_f = 20_000.0_f64;
        let log_min = min_f.log10();
        let log_max = max_f.log10();
        let x_for_freq = |freq: f64| {
            rect.left()
                + ((freq.max(min_f).min(max_f).log10() - log_min) / (log_max - log_min)) as f32
                    * rect.width()
        };
        let y_for_db = |db: f64| {
            rect.center().y - (db.clamp(-24.0, 24.0) / 24.0) as f32 * rect.height() * 0.42
        };
        for (freq, label) in [
            (20.0, "20"),
            (100.0, "100"),
            (1_000.0, "1k"),
            (10_000.0, "10k"),
        ] {
            let x = x_for_freq(freq);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.0_f32, theme::GRID),
            );
            painter.text(
                egui::pos2(x + 3.0, rect.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::monospace(8.0),
                theme::TEXT_MUTED,
            );
        }
        for db in [-12.0, 0.0, 12.0] {
            let y = y_for_db(db);
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(
                    1.0_f32,
                    if db == 0.0 {
                        theme::GRID_STRONG
                    } else {
                        theme::GRID
                    },
                ),
            );
        }
        let values: BTreeMap<ParamId, ParamValue> = device
            .param_values
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect();
        let sample_rate = 48_000.0;
        let mut points = Vec::with_capacity(180);
        for index in 0..=180u32 {
            let t = f64::from(index) / 180.0;
            let freq = 10.0_f64.powf(log_min + t * (log_max - log_min));
            let magnitude = notwig_dsp::stock::eq_magnitude_response(&values, sample_rate, freq);
            let db = 20.0 * magnitude.max(1e-9).log10();
            points.push(egui::pos2(x_for_freq(freq), y_for_db(db)));
        }
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.8_f32, theme::CLIP_MIDI),
        ));
        for band in 0..4 {
            let enabled = !matches!(
                values.get(&notwig_dsp::stock::band_param(band, 0)),
                Some(ParamValue::Bool(false))
            );
            if !enabled {
                continue;
            }
            let freq = values
                .get(&notwig_dsp::stock::band_param(band, 2))
                .and_then(ParamValue::as_f64)
                .unwrap_or(1_000.0);
            let kind = values
                .get(&notwig_dsp::stock::band_param(band, 1))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0) as u32;
            let gain = if kind == 3 || kind == 4 {
                0.0
            } else {
                values
                    .get(&notwig_dsp::stock::band_param(band, 3))
                    .and_then(ParamValue::as_f64)
                    .unwrap_or(0.0)
            };
            painter.circle_filled(
                egui::pos2(x_for_freq(freq), y_for_db(gain)),
                4.0,
                theme::SELECTION,
            );
        }
        ui.add_space(4.0);
    }

    fn param_editor(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
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
        for param in &spec.params {
            self.param_control(ui, device, param);
        }
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

fn spec_name(ui: &NotwigUi, device: &DeviceInstance) -> String {
    ui.registry
        .resolve(&device.type_id, device.version)
        .map_or_else(|| device.type_id.clone(), |spec| spec.name.clone())
}

fn card_width(type_id: &str) -> f32 {
    if type_id == notwig_dsp::stock::EQ_TYPE_ID {
        360.0
    } else {
        280.0
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

#[allow(dead_code)]
fn _keep(_: &NotwigUi, _: &ProjectDoc, _: &RuntimeView, _: &DeviceInstance) {}
