use std::f32::consts::PI;

use egui::{Align, Color32, CornerRadius, FontId, Id, Layout, Rect, Sense, Stroke, Vec2};
use notwig_core::dsp::DeviceSpec;
use notwig_core::{DeviceInstance, DeviceKind, TrackId};

use crate::icons::Icon;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

use super::bracket::chain_tab;
use super::plugin_kind::is_plugin_device;
use super::{DEVICE_CARD_HEIGHT, DeviceRendererKind, HANDLE_WIDTH, card_width, device_renderer};

impl NotwigUi {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn device_card(
        &mut self,
        ui: &mut egui::Ui,
        doc: &notwig_core::ProjectDoc,
        runtime: &RuntimeView,
        device: &DeviceInstance,
        track_id: TrackId,
        depth: usize,
    ) -> Rect {
        let selected = self.state.selection.devices.contains(&device.id);
        let collapsed = self.collapsed_devices.contains(&device.id);
        let spec = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned();
        let expanded_width = card_width(spec.as_ref());
        let width = if collapsed {
            HANDLE_WIDTH
        } else {
            expanded_width
        };
        let body_height = DEVICE_CARD_HEIGHT - 2.0;
        let response = ui
            .vertical(|ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::GRID))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::ZERO)
                    .show(ui, |ui| {
                        ui.push_id(("device-card", device.id), |ui| {
                            ui.set_width(width);
                            ui.set_min_height(body_height);
                            ui.horizontal_top(|ui| {
                                self.device_handle(
                                    ui,
                                    device,
                                    track_id,
                                    body_height,
                                    selected,
                                    collapsed,
                                );
                                if !collapsed {
                                    self.device_body(
                                        ui,
                                        doc,
                                        runtime,
                                        device,
                                        spec.as_ref(),
                                        body_height,
                                        depth,
                                    );
                                }
                            });
                        });
                    });
            })
            .response
            .rect;
        ui.add_space(2.0);
        response
    }

    fn device_handle(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        track_id: TrackId,
        height: f32,
        selected: bool,
        collapsed: bool,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(HANDLE_WIDTH, height), Sense::click());
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            if collapsed {
                CornerRadius::same(4)
            } else {
                CornerRadius {
                    nw: 4,
                    sw: 4,
                    ne: 0,
                    se: 0,
                }
            },
            if selected {
                Color32::from_rgb(82, 86, 96)
            } else {
                Color32::from_rgb(41, 43, 50)
            },
        );
        let power_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 14.0),
            Vec2::splat(22.0),
        );
        let power = ui.interact(
            power_rect,
            Id::new(("device-power", device.id)),
            Sense::click(),
        );
        crate::icons::paint(
            painter,
            power_rect.shrink(3.0),
            Icon::Power,
            if device.bypass {
                theme::TEXT_MUTED
            } else {
                theme::ACCENT
            },
        );
        if power.clicked() {
            self.emit(UiIntent::SetDeviceBypass {
                device_id: device.id,
                bypass: !device.bypass,
            });
        }
        let name = self
            .device_name(device)
            .to_uppercase()
            .chars()
            .take(20)
            .collect::<String>();
        let name_color = if selected {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        };
        let galley = painter.layout_no_wrap(name, FontId::monospace(9.0), name_color);
        let name_center = egui::pos2(rect.center().x, rect.top() + height * 0.55);
        let name_pos = egui::pos2(
            name_center.x - galley.size().y / 2.0,
            name_center.y + galley.size().x / 2.0,
        );
        painter
            .add(egui::epaint::TextShape::new(name_pos, galley, name_color).with_angle(-PI / 2.0));
        let mut window_clicked = false;
        if is_plugin_device(device) {
            let window_rect = Rect::from_center_size(
                egui::pos2(rect.center().x, rect.bottom() - 15.0),
                Vec2::splat(18.0),
            );
            let window = ui
                .interact(
                    window_rect,
                    Id::new(("device-window", device.id)),
                    Sense::click(),
                )
                .on_hover_text("Open plugin window");
            let color = if window.hovered() {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            };
            crate::icons::paint(
                painter,
                window_rect,
                crate::icons::Icon::ExternalLink,
                color,
            );
            if window.clicked() {
                window_clicked = true;
                self.emit(UiIntent::OpenPluginWindow {
                    device_id: device.id,
                });
            }
        }
        if response.double_clicked() && !power.clicked() && !window_clicked {
            if self.collapsed_devices.contains(&device.id) {
                self.collapsed_devices.remove(&device.id);
            } else {
                self.collapsed_devices.insert(device.id);
            }
        } else if response.clicked() && !power.clicked() && !window_clicked {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.state
                .selection
                .select_device(device.id, track_id, additive);
        }
        response.context_menu(|ui| {
            if ui.button("Remove device").clicked() {
                self.emit(UiIntent::RemoveDevice(device.id));
                ui.close();
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn device_body(
        &mut self,
        ui: &mut egui::Ui,
        doc: &notwig_core::ProjectDoc,
        runtime: &RuntimeView,
        device: &DeviceInstance,
        spec: Option<&DeviceSpec>,
        height: f32,
        depth: usize,
    ) {
        let body_width = card_width(spec) - HANDLE_WIDTH - 2.0;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(body_width - 8.0, height - 4.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.spacing_mut().interact_size.y = 16.0;
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if device.kind == DeviceKind::Container {
                            let expanded = device
                                .child_chains
                                .first()
                                .is_some_and(|chain| self.expanded_chains.contains(chain));
                            if chain_tab(ui, expanded, depth + 1) {
                                if let Some(child_chain) = device.child_chains.first().copied() {
                                    if expanded {
                                        self.expanded_chains.remove(&child_chain);
                                    } else {
                                        self.expanded_chains.insert(child_chain);
                                    }
                                }
                            }
                            ui.add_space(4.0);
                        }
                        let renderer = device_renderer(spec);
                        // Extension devices own their body. Never fall back to
                        // the generic parameter editor: toggling between the
                        // two makes meters and custom visuals flash.
                        if matches!(&renderer, DeviceRendererKind::Extension(_)) {
                            if let Some(spec) = spec {
                                self.draw_extension_device_panel(
                                    ui,
                                    device,
                                    spec,
                                    doc.meta.sample_rate.value(),
                                    runtime
                                        .device_meters
                                        .get(&device.id)
                                        .copied()
                                        .unwrap_or_default(),
                                    Vec2::new(body_width - 8.0, height - 4.0),
                                );
                            }
                        } else {
                            match (renderer, spec) {
                                (DeviceRendererKind::Eq, _) => {
                                    self.eq_card_body_from_panel(
                                        ui,
                                        device,
                                        f64::from(doc.meta.sample_rate.value()),
                                    );
                                }
                                (DeviceRendererKind::Tool, Some(spec)) => {
                                    self.tool_body(ui, device, spec, body_width);
                                }
                                (DeviceRendererKind::Chain, Some(spec)) => {
                                    self.chain_body(ui, device, spec);
                                }
                                _ => self.param_editor_from_panel(ui, device),
                            }
                        }
                        let _ = runtime;
                    },
                );
            });
    }
}
