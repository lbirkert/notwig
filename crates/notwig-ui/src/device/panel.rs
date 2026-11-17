use std::collections::BTreeSet;
use std::f32::consts::PI;

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use notwig_core::{
    DeviceChainId, DeviceId, DeviceInstance, DeviceKind, ParamId, ParamValue, ProjectDoc, TrackId,
};
use notwig_dsp::ParamSpec;

use crate::icons::{Icon, icon_button_sized};
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_param;

const DEVICE_CARD_HEIGHT: f32 = 286.0;
const DEVICE_PANEL_HEIGHT: f32 = 340.0;
const HANDLE_WIDTH: f32 = 38.0;

impl NotwigUi {
    pub(crate) fn device_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        egui::TopBottomPanel::bottom("notwig_devices")
            .resizable(false)
            .exact_height(DEVICE_PANEL_HEIGHT)
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
                let main_chain = track.main_chain;
                let end_index = doc
                    .chains
                    .get(&main_chain)
                    .map_or(0, |chain| chain.devices.len());
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
                        if ui.button("+ Device").clicked() {
                            self.open_device_browser(track_id, main_chain, end_index);
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::horizontal()
                    .id_salt("device_chain")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            self.device_chain(ui, doc, runtime, track_id, main_chain, 0);
                        });
                    });
            });
    }

    fn device_chain(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        track_id: TrackId,
        chain_id: DeviceChainId,
        depth: usize,
    ) {
        let devices = doc
            .chains
            .get(&chain_id)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        if devices.is_empty() {
            ui.vertical(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("No devices in this chain.")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            });
        }
        for (index, device_id) in devices.iter().enumerate() {
            self.insertion_point(ui, track_id, chain_id, index);
            let Some(device) = doc.devices.get(device_id).cloned() else {
                continue;
            };
            self.device_card(ui, doc, runtime, &device, track_id, depth);
            if device.kind == DeviceKind::Container {
                if let Some(child_chain) = device.child_chains.first().copied() {
                    if self.expanded_chains.contains(&child_chain) {
                        self.device_chain(ui, doc, runtime, track_id, child_chain, depth + 1);
                    }
                }
            }
        }
        self.insertion_point(ui, track_id, chain_id, devices.len());
    }

    fn insertion_point(
        &mut self,
        ui: &mut egui::Ui,
        track_id: TrackId,
        chain_id: DeviceChainId,
        index: usize,
    ) {
        ui.allocate_ui_with_layout(
            Vec2::new(26.0, DEVICE_CARD_HEIGHT),
            Layout::top_down(Align::Center),
            |ui| {
                ui.add_space((DEVICE_CARD_HEIGHT - 24.0) / 2.0);
                if icon_button_sized(ui, Icon::Plus, Vec2::splat(22.0), false, "Insert device")
                    .clicked()
                {
                    self.open_device_browser(track_id, chain_id, index);
                }
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn device_card(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        device: &DeviceInstance,
        track_id: TrackId,
        depth: usize,
    ) {
        let selected = self.state.selection.devices.contains(&device.id);
        let width = card_width(device);
        let bar_space = 2.0 + (depth + 1) as f32 * 4.0;
        egui::Frame::new()
            .fill(theme::PANEL)
            .stroke(Stroke::new(
                1.0_f32,
                if selected { theme::ACCENT } else { theme::GRID },
            ))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(6, 6))
            .show(ui, |ui| {
                ui.push_id(("device-card", device.id), |ui| {
                    ui.set_width(width);
                    ui.set_min_height(DEVICE_CARD_HEIGHT - 12.0);
                    nest_bars(ui, width, depth, bar_space);
                    let body_height = DEVICE_CARD_HEIGHT - 12.0 - bar_space;
                    ui.horizontal_top(|ui| {
                        self.device_handle(ui, device, track_id, body_height, selected);
                        self.device_body(ui, doc, runtime, device, body_height);
                    });
                });
            });
        ui.add_space(2.0);
    }

    fn device_handle(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        track_id: TrackId,
        height: f32,
        selected: bool,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(HANDLE_WIDTH, height), Sense::click());
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            },
            if selected {
                theme::ACCENT_DIM
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
        let folder_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 38.0),
            Vec2::splat(14.0),
        );
        crate::icons::paint(painter, folder_rect, Icon::Folder, theme::TEXT_MUTED);
        let name = self.device_name(device).to_uppercase();
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 54.0),
            Align2::CENTER_TOP,
            name.chars().take(10).collect::<String>(),
            FontId::monospace(8.5),
            if selected {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            },
        );
        let info_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 14.0),
            Vec2::splat(13.0),
        );
        crate::icons::paint(painter, info_rect, Icon::Info, theme::TEXT_MUTED);
        if response.clicked() && !power.clicked() {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.state
                .selection
                .select_device(device.id, track_id, additive);
        }
    }

    fn device_body(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        device: &DeviceInstance,
        height: f32,
    ) {
        ui.allocate_ui_with_layout(
            Vec2::new(card_width(device) - HANDLE_WIDTH - 14.0, height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.spacing_mut().interact_size.y = 16.0;
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    if device.kind == DeviceKind::Container {
                        let expanded = device
                            .child_chains
                            .first()
                            .is_some_and(|chain| self.expanded_chains.contains(chain));
                        if ui.selectable_label(expanded, "CHAIN").clicked() {
                            if let Some(child_chain) = device.child_chains.first().copied() {
                                if expanded {
                                    self.expanded_chains.remove(&child_chain);
                                } else {
                                    self.expanded_chains.insert(child_chain);
                                }
                            }
                        }
                    } else {
                        ui.label(RichText::new(self.device_name(device)).strong());
                    }
                    ui.label(
                        RichText::new(device_kind_label(device.kind))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            self.emit(UiIntent::RemoveDevice(device.id));
                        }
                    });
                });
                ui.separator();
                if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
                    self.eq_card_body(ui, device, f64::from(doc.meta.sample_rate.value()));
                } else if device.kind == DeviceKind::Container {
                    self.chain_body(ui, device);
                } else {
                    self.param_editor(ui, device);
                }
                let _ = runtime;
            },
        );
    }

    fn chain_body(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        let Some(spec) = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned()
        else {
            ui.label(
                RichText::new("No parameter metadata.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        };
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            for (id, label) in [
                (notwig_dsp::stock::p_input_gain(), "Wet Gain"),
                (notwig_dsp::stock::p_dry_wet(), "Mix"),
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
                if knob(
                    ui,
                    &mut norm,
                    label,
                    &format_param(param.unit, param.physical(&current)),
                ) {
                    self.emit(UiIntent::SetParam {
                        device_id: device.id,
                        param_id: param.id,
                        value: param.value_from_norm(notwig_core::Norm01::clamped(norm)),
                    });
                }
                ui.add_space(6.0);
            }
        });
    }
}

fn card_width(device: &DeviceInstance) -> f32 {
    if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
        620.0
    } else if device.type_id == notwig_dsp::stock::CHAIN_TYPE_ID {
        190.0
    } else if device.type_id == notwig_dsp::stock::TOOL_TYPE_ID {
        300.0
    } else {
        340.0
    }
}

fn nest_bars(ui: &mut egui::Ui, width: f32, depth: usize, bar_space: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, bar_space), Sense::hover());
    let painter = ui.painter();
    for level in 0..=depth {
        let y = rect.top() + 2.0 + level as f32 * 4.0;
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(rect.left(), y), Vec2::new(width, 3.0)),
            CornerRadius::same(1),
            nesting_color(level),
        );
    }
}

fn nesting_color(depth: usize) -> Color32 {
    const COLORS: [Color32; 5] = [
        Color32::from_rgb(88, 148, 205),
        Color32::from_rgb(104, 176, 112),
        Color32::from_rgb(204, 156, 72),
        Color32::from_rgb(164, 112, 196),
        Color32::from_rgb(80, 176, 180),
    ];
    COLORS[depth.min(COLORS.len() - 1)]
}

fn device_kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Instrument => "INSTRUMENT",
        DeviceKind::AudioFx => "AUDIO FX",
        DeviceKind::NoteFx => "NOTE FX",
        DeviceKind::Container => "CONTAINER",
    }
}

fn knob(ui: &mut egui::Ui, norm: &mut f32, label: &str, value: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(78.0, 84.0), Sense::drag());
    let changed = response.dragged();
    if changed {
        *norm = (*norm - response.drag_delta().y * 0.005).clamp(0.0, 1.0);
    }
    let painter = ui.painter();
    let center = egui::pos2(rect.center().x, rect.top() + 27.0);
    let radius = 20.0;
    painter.circle_filled(center, radius + 4.0, Color32::from_rgb(27, 29, 35));
    painter.circle_stroke(center, radius, Stroke::new(2.0_f32, theme::GRID));
    let start = -PI * 0.75;
    let end = PI * 0.75;
    let angle = start + (end - start) * norm.clamp(0.0, 1.0);
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let a = start + (angle - start) * t;
        points.push(egui::pos2(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    painter.add(egui::Shape::line(
        points,
        Stroke::new(3.0_f32, theme::ACCENT),
    ));
    painter.line_segment(
        [
            center,
            egui::pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ),
        ],
        Stroke::new(2.0_f32, Color32::WHITE),
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 57.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(11.0),
        theme::TEXT_MUTED,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 1.0),
        Align2::CENTER_BOTTOM,
        value,
        FontId::monospace(9.5),
        theme::ACCENT,
    );
    changed
}

#[allow(dead_code)]
fn _keep(
    _: ParamId,
    _: ParamValue,
    _: ParamSpec,
    _: Rect,
    _: StrokeKind,
    _: BTreeSet<DeviceChainId>,
    _: DeviceId,
) {
}
