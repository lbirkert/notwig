use egui::{Align, Color32, CornerRadius, Layout, RichText, Sense, Stroke, Vec2};
use notwig_core::{DeviceId, DeviceInstance, DeviceKind, ProjectDoc, TrackId};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    pub(crate) fn device_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        egui::TopBottomPanel::bottom("notwig_devices")
            .resizable(false)
            .exact_height(360.0)
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
                        RichText::new("DEVICE PANEL")
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
                let card_height = (ui.available_height() - 2.0).max(210.0);
                egui::ScrollArea::horizontal()
                    .id_salt("device_chain")
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
                                    self.device_card(ui, doc, *device_id, track_id, card_height);
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
        card_height: f32,
    ) {
        let Some(device) = doc.devices.get(&device_id) else {
            return;
        };
        let selected = self.state.selection.devices.contains(&device_id);
        let width = (card_height * 1.28).clamp(300.0, 470.0);
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
                            if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
                                self.eq_card_body(
                                    ui,
                                    device,
                                    f64::from(doc.meta.sample_rate.value()),
                                );
                            } else {
                                self.param_editor(ui, device);
                            }
                        },
                    );
                });
            });
        ui.add_space(6.0);
    }
}

fn spec_name(ui: &NotwigUi, device: &DeviceInstance) -> String {
    ui.registry
        .resolve(&device.type_id, device.version)
        .map_or_else(|| device.type_id.clone(), |spec| spec.name.clone())
}

fn device_kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Instrument => "INSTRUMENT",
        DeviceKind::AudioFx => "AUDIO FX",
        DeviceKind::NoteFx => "NOTE FX",
        DeviceKind::Container => "CONTAINER",
    }
}
