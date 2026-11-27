use std::collections::BTreeSet;

use egui::{Rect, RichText, Sense, StrokeKind, Vec2};
use notwig_core::dsp::ParamSpec;
use notwig_core::{
    DeviceChainId, DeviceId, DeviceInstance, DeviceKind, ParamId, ParamValue, ProjectDoc, TrackId,
};

use crate::icons::{Icon, icon_button_sized};
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

mod bracket;
mod browser;
mod card;
mod eq;
mod extension;
mod knob;
pub(crate) mod panel_render;
mod params;
mod plugin_kind;
mod renderer;
mod stock_body;

use bracket::{Bracket, draw_bracket};

pub(crate) use browser::DeviceBrowserState;
pub(crate) use eq::EqDrag;
pub(crate) use extension::theme_snapshot;
pub(crate) use renderer::{DeviceRendererKind, card_width, device_renderer};

const DEVICE_CARD_HEIGHT: f32 = 220.0;
const DEVICE_PANEL_HEIGHT: f32 = 260.0;
const OVERLINE_SPACE: f32 = 10.0;
const HANDLE_WIDTH: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceDrag {
    pub device_id: DeviceId,
    pub source_chain: DeviceChainId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DeviceInsertion {
    pub chain_id: DeviceChainId,
    pub index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceDropLocation {
    chain_id: DeviceChainId,
    index: usize,
    lane_track: Option<TrackId>,
}

impl NotwigUi {
    pub(crate) fn device_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let open = super::panel::bottom_panel(
            ctx,
            super::panel::PanelConfig {
                id: "notwig_devices",
                resizable: false,
                default_height: DEVICE_PANEL_HEIGHT,
                min_height: DEVICE_PANEL_HEIGHT,
                max_height: DEVICE_PANEL_HEIGHT,
                collapse_height: 0.0,
                handle_id: "device-panel",
                handle_tooltip: "Drag down to close",
            },
            |ui| {
                self.device_insertions.clear();
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
            let card_rect = self.device_card(ui, doc, runtime, &device, track_id, chain_id, depth);
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
        self.device_insertions.push(DeviceInsertion {
            chain_id,
            index,
            rect,
        });
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

    fn device_drop_location(
        &self,
        doc: &ProjectDoc,
        pointer: egui::Pos2,
    ) -> Option<DeviceDropLocation> {
        if let Some(insertion) = self
            .device_insertions
            .iter()
            .find(|insertion| insertion.rect.contains(pointer))
        {
            return Some(DeviceDropLocation {
                chain_id: insertion.chain_id,
                index: insertion.index,
                lane_track: None,
            });
        }
        if !self.arranger_rect.contains(pointer) {
            return None;
        }
        let track_id = self.track_at(pointer.y)?;
        let track = doc.tracks.get(&track_id)?;
        let index = doc
            .chains
            .get(&track.main_chain)
            .map_or(0, |chain| chain.devices.len());
        Some(DeviceDropLocation {
            chain_id: track.main_chain,
            index,
            lane_track: Some(track_id),
        })
    }

    pub(crate) fn draw_device_drag_preview(&self, ctx: &egui::Context, doc: &ProjectDoc) {
        if self.device_drag.is_none() {
            return;
        }
        let Some(pointer) = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()))
        else {
            return;
        };
        let overlay = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("device-drag-preview"),
        ));
        let Some(location) = self.device_drop_location(doc, pointer) else {
            return;
        };
        if let Some(track_id) = location.lane_track {
            if let Some(row) = self.row_rect(track_id) {
                overlay.rect_stroke(
                    row.shrink(1.0),
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(2.0_f32, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }
            return;
        }
        if let Some(insertion) = self.device_insertions.iter().find(|insertion| {
            insertion.chain_id == location.chain_id && insertion.index == location.index
        }) {
            let x = insertion.rect.center().x;
            overlay.line_segment(
                [
                    egui::pos2(x, insertion.rect.top() + 8.0),
                    egui::pos2(x, insertion.rect.bottom() - 8.0),
                ],
                egui::Stroke::new(2.0_f32, theme::ACCENT),
            );
        }
    }

    pub(crate) fn finish_device_drag(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        let Some(drag) = self.device_drag else {
            return;
        };
        if !ctx.input(|input| input.pointer.any_released()) {
            return;
        }
        let pointer = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()));
        if let Some(pointer) = pointer
            && let Some(location) = self.device_drop_location(doc, pointer)
        {
            let source_index = doc
                .chains
                .get(&drag.source_chain)
                .and_then(|chain| chain.devices.iter().position(|id| *id == drag.device_id));
            let mut index = location.index;
            if let Some(source_index) = source_index {
                if location.chain_id == drag.source_chain && source_index < index {
                    index = index.saturating_sub(1);
                }
                if location.chain_id == drag.source_chain && index == source_index {
                    self.device_drag = None;
                    return;
                }
            }
            self.emit(UiIntent::MoveDevice {
                device_id: drag.device_id,
                to_chain: location.chain_id,
                index,
            });
        }
        self.device_drag = None;
    }
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

impl NotwigUi {
    pub(crate) fn eq_card_body_from_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        sample_rate: f64,
    ) {
        self.eq_card_body(ui, device, sample_rate);
    }

    pub(crate) fn param_editor_from_panel(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        self.param_editor(ui, device);
    }
}

pub mod test_api {
    pub fn plugin_format_tag(type_id: &str) -> Option<&'static str> {
        super::plugin_kind::format_tag(type_id)
    }

    pub fn begin_device_drag(
        ui: &mut crate::shell::NotwigUi,
        device_id: notwig_core::DeviceId,
        source_chain: notwig_core::DeviceChainId,
    ) {
        ui.device_drag = Some(super::DeviceDrag {
            device_id,
            source_chain,
        });
    }

    pub fn insertion_rect(
        ui: &crate::shell::NotwigUi,
        chain_id: notwig_core::DeviceChainId,
        index: usize,
    ) -> Option<egui::Rect> {
        ui.device_insertions
            .iter()
            .find(|insertion| insertion.chain_id == chain_id && insertion.index == index)
            .map(|insertion| insertion.rect)
    }
}
