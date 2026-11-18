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

const DEVICE_CARD_HEIGHT: f32 = 220.0;
const DEVICE_PANEL_HEIGHT: f32 = 260.0;
const OVERLINE_SPACE: f32 = 10.0;
const HANDLE_WIDTH: f32 = 32.0;

#[derive(Clone, Copy)]
struct Bracket {
    left: f32,
    right: f32,
    y: f32,
    depth: usize,
}

impl NotwigUi {
    pub(crate) fn device_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let open = super::bottom_panel(
            ctx,
            super::PanelConfig {
                id: "notwig_devices",
                resizable: false,
                default_height: DEVICE_PANEL_HEIGHT,
                min_height: DEVICE_PANEL_HEIGHT,
                max_height: DEVICE_PANEL_HEIGHT,
                handle_id: "device-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                let Some(track_id) = self.selected_track(doc) else {
                    ui.label(
                        RichText::new("Select an Arrangement Lane to edit.")
                            .color(theme::TEXT_MUTED),
                    );
                    return true;
                };
                let Some(track) = doc.tracks.get(&track_id) else {
                    return true;
                };
                let main_chain = track.main_chain;
                egui::ScrollArea::horizontal()
                    .id_salt("device_chain")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.add_space(OVERLINE_SPACE);
                            let mut brackets = Vec::new();
                            ui.horizontal_top(|ui| {
                                self.device_chain(
                                    ui,
                                    doc,
                                    runtime,
                                    track_id,
                                    main_chain,
                                    0,
                                    &mut brackets,
                                );
                            });
                            brackets.sort_by_key(|bracket| bracket.depth);
                            for bracket in brackets {
                                draw_bracket(
                                    ui,
                                    bracket.left,
                                    bracket.right,
                                    bracket.y,
                                    bracket.depth,
                                );
                            }
                        });
                    });
                true
            },
        );
        if !open {
            self.close_editor();
        }
    }

    fn device_chain(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        track_id: TrackId,
        chain_id: DeviceChainId,
        depth: usize,
        brackets: &mut Vec<Bracket>,
    ) {
        let devices = doc
            .chains
            .get(&chain_id)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        let mut first_element: Option<Rect> = None;
        for (index, device_id) in devices.iter().enumerate() {
            let plus = self.insertion_point(ui, track_id, chain_id, index);
            first_element.get_or_insert(plus);
            let Some(device) = doc.devices.get(device_id).cloned() else {
                continue;
            };
            let card_rect = self.device_card(ui, doc, runtime, &device, track_id, depth);
            first_element.get_or_insert(card_rect);
            if !self.collapsed_devices.contains(&device.id) && device.kind == DeviceKind::Container
            {
                if let Some(child_chain) = device.child_chains.first().copied() {
                    if self.expanded_chains.contains(&child_chain) {
                        self.device_chain(
                            ui,
                            doc,
                            runtime,
                            track_id,
                            child_chain,
                            depth + 1,
                            brackets,
                        );
                    }
                }
            }
        }
        let trailing = self.insertion_point(ui, track_id, chain_id, devices.len());
        first_element.get_or_insert(trailing);
        if depth > 0 {
            if let Some(first) = first_element {
                brackets.push(Bracket {
                    left: first.left(),
                    right: trailing.right(),
                    y: first.top() - 8.0,
                    depth,
                });
            }
        }
    }

    fn insertion_point(
        &mut self,
        ui: &mut egui::Ui,
        track_id: TrackId,
        chain_id: DeviceChainId,
        index: usize,
    ) -> Rect {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(18.0, DEVICE_CARD_HEIGHT), Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((DEVICE_CARD_HEIGHT - 18.0) / 2.0);
                if icon_button_sized(ui, Icon::Plus, Vec2::splat(16.0), false, "Insert device")
                    .clicked()
                {
                    self.open_device_browser(track_id, chain_id, index);
                }
            });
        });
        rect
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
    ) -> Rect {
        let selected = self.state.selection.devices.contains(&device.id);
        let collapsed = self.collapsed_devices.contains(&device.id);
        let expanded_width = card_width(device);
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
                                    self.device_body(ui, doc, runtime, device, body_height, depth);
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
            let icon_rect = window_rect.shrink(3.0);
            let color = if window.hovered() {
                Color32::WHITE
            } else {
                theme::TEXT_MUTED
            };
            painter.rect_stroke(
                icon_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, color),
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(icon_rect.left() + 3.0, icon_rect.bottom() - 3.0),
                    egui::pos2(icon_rect.right() - 3.0, icon_rect.top() + 3.0),
                ],
                Stroke::new(1.2_f32, color),
            );
            painter.line_segment(
                [
                    egui::pos2(icon_rect.right() - 6.0, icon_rect.top() + 3.0),
                    egui::pos2(icon_rect.right() - 3.0, icon_rect.top() + 3.0),
                ],
                Stroke::new(1.2_f32, color),
            );
            painter.line_segment(
                [
                    egui::pos2(icon_rect.right() - 3.0, icon_rect.top() + 3.0),
                    egui::pos2(icon_rect.right() - 3.0, icon_rect.top() + 6.0),
                ],
                Stroke::new(1.2_f32, color),
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

    fn device_body(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        device: &DeviceInstance,
        height: f32,
        depth: usize,
    ) {
        let body_width = card_width(device) - HANDLE_WIDTH - 2.0;
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
                        if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
                            self.eq_card_body_from_panel(
                                ui,
                                device,
                                f64::from(doc.meta.sample_rate.value()),
                            );
                        } else if device.kind == DeviceKind::Container {
                            self.chain_body(ui, device);
                        } else if device.type_id == notwig_dsp::stock::TOOL_TYPE_ID {
                            self.tool_body(ui, device, body_width);
                        } else {
                            self.param_editor_from_panel(ui, device);
                        }
                        let _ = runtime;
                    },
                );
            });
    }

    fn tool_body(&mut self, ui: &mut egui::Ui, device: &DeviceInstance, width: f32) {
        let Some(spec) = self
            .registry
            .resolve(&device.type_id, device.version)
            .cloned()
        else {
            return;
        };
        let button_width = ((width - 32.0) / 3.0).max(42.0);
        ui.horizontal(|ui| {
            for (id, label) in [
                (notwig_dsp::stock::p_polarity_l(), "L−"),
                (notwig_dsp::stock::p_swap_lr(), "Swap L/R"),
                (notwig_dsp::stock::p_polarity_r(), "R−"),
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
            (notwig_dsp::stock::p_input_gain(), "Volume"),
            (notwig_dsp::stock::p_output_gain(), "Gain"),
            (notwig_dsp::stock::p_pan(), "Pan"),
            (notwig_dsp::stock::p_width(), "Width"),
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
                        id if id == notwig_dsp::stock::p_pan() => {
                            format!("{:.0} %", param.physical(&current) * 100.0)
                        }
                        id if id == notwig_dsp::stock::p_width() => {
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
                            value: param.value_from_norm(notwig_core::Norm01::clamped(norm)),
                        });
                    }
                    ui.add_space(gap);
                }
            });
        }
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
            ui.add_space(6.0);
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
                        value: param.value_from_norm(notwig_core::Norm01::clamped(norm)),
                    });
                }
                ui.add_space(2.0);
            }
        });
    }
}

fn is_plugin_device(device: &DeviceInstance) -> bool {
    device.type_id.starts_with("vst3.") || device.type_id.starts_with("au.")
}

fn card_width(device: &DeviceInstance) -> f32 {
    if device.type_id == notwig_dsp::stock::EQ_TYPE_ID {
        560.0
    } else if device.type_id == notwig_dsp::stock::CHAIN_TYPE_ID {
        150.0
    } else {
        300.0
    }
}

fn draw_bracket(ui: &egui::Ui, left: f32, right: f32, y: f32, depth: usize) {
    draw_bracket_lines(ui.painter(), left, right, y, nesting_color(depth));
}

fn draw_bracket_lines(painter: &egui::Painter, left: f32, right: f32, y: f32, color: Color32) {
    let stroke = Stroke::new(3.0_f32, color);
    let left = left + 2.0;
    let right = right - 2.0;
    painter.line_segment(
        [egui::pos2(left, y + 7.0), egui::pos2(left, y + 2.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(left, y + 2.0), egui::pos2(right, y + 2.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(right, y + 2.0), egui::pos2(right, y + 7.0)],
        stroke,
    );
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

fn chain_tab(ui: &mut egui::Ui, expanded: bool, depth: usize) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(84.0, 24.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        if response.hovered() {
            theme::GRID_STRONG
        } else {
            Color32::from_rgb(20, 21, 25)
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        Stroke::new(1.0_f32, theme::GRID),
        StrokeKind::Inside,
    );
    let bracket_left = rect.center().x - 22.0;
    let bracket_right = rect.center().x + 22.0;
    draw_bracket_lines(
        painter,
        bracket_left,
        bracket_right,
        rect.center().y - 7.0,
        nesting_color(depth),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "CHAIN",
        FontId::monospace(10.0),
        if expanded {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        },
    );
    response.clicked()
}

fn knob(
    ui: &mut egui::Ui,
    id: Id,
    norm: &mut f32,
    default_norm: f32,
    value: &str,
    label: &str,
    bipolar: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(78.0, 84.0), Sense::click_and_drag());
    let mut changed = false;
    if let Some(value) = crate::widgets::value_drag(
        ui,
        &response,
        id,
        f64::from(*norm),
        f64::from(default_norm),
        0.0,
        1.0,
        0.005,
        true,
    ) {
        *norm = value as f32;
        changed = true;
    }
    let painter = ui.painter();
    let center = egui::pos2(rect.center().x, rect.top() + 27.0);
    let radius = 20.0;
    painter.circle_filled(center, radius + 4.0, Color32::from_rgb(27, 29, 35));
    painter.circle_stroke(center, radius, Stroke::new(2.0_f32, theme::GRID));
    let rotation = -PI / 2.0;
    let start = -PI * 0.75 + rotation;
    let end = PI * 0.75 + rotation;
    let angle = start + (end - start) * norm.clamp(0.0, 1.0);
    let arc_start = if bipolar { rotation } else { start };
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let a = arc_start + (angle - arc_start) * t;
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
    if bipolar {
        painter.line_segment(
            [
                egui::pos2(center.x, center.y - radius - 4.0),
                egui::pos2(center.x, center.y - radius - 1.0),
            ],
            Stroke::new(2.0_f32, theme::TEXT_MUTED),
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 57.0),
        Align2::CENTER_TOP,
        value,
        FontId::monospace(9.5),
        theme::ACCENT,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 1.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(11.0),
        theme::TEXT_MUTED,
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
