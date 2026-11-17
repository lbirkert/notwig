use egui::{RichText, Vec2};
use notwig_core::{DeviceChainId, DeviceKind, ProjectDoc, TrackId};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceBrowserState {
    pub track_id: TrackId,
    pub chain_id: DeviceChainId,
    pub index: usize,
    pub search: String,
}

impl NotwigUi {
    pub(crate) fn open_device_browser(
        &mut self,
        track_id: TrackId,
        chain_id: DeviceChainId,
        index: usize,
    ) {
        self.device_browser = Some(DeviceBrowserState {
            track_id,
            chain_id,
            index,
            search: String::new(),
        });
    }

    pub(crate) fn device_browser_window(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(state) = self.device_browser.clone() else {
            return;
        };
        let mut open = true;
        let mut selected: Option<(String, DeviceKind, bool)> = None;
        egui::Window::new("Add Device")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(340.0)
            .default_height(440.0)
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.device_browser.as_mut().unwrap().search)
                        .hint_text("Search devices")
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                let search = self
                    .device_browser
                    .as_ref()
                    .map(|state| state.search.to_lowercase())
                    .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("STOCK")
                                .small()
                                .strong()
                                .color(theme::TEXT_MUTED),
                        );
                        for (name, type_id, kind) in [
                            ("Tool", notwig_dsp::stock::TOOL_TYPE_ID, DeviceKind::AudioFx),
                            ("EQ", notwig_dsp::stock::EQ_TYPE_ID, DeviceKind::AudioFx),
                            (
                                "Chain",
                                notwig_dsp::stock::CHAIN_TYPE_ID,
                                DeviceKind::Container,
                            ),
                        ] {
                            if !search.is_empty()
                                && !name.to_lowercase().contains(&search)
                                && !type_id.contains(&search)
                            {
                                continue;
                            }
                            let response = ui.add_sized(
                                Vec2::new(ui.available_width(), 26.0),
                                egui::Button::new(
                                    RichText::new(format!("{name}  ·  {}", kind_label(kind)))
                                        .color(theme::TEXT_MUTED),
                                )
                                .frame(false),
                            );
                            if response.clicked() {
                                selected = Some((type_id.into(), kind, true));
                            }
                        }
                        if !runtime.plugins.is_empty() {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("PLUGINS")
                                    .small()
                                    .strong()
                                    .color(theme::TEXT_MUTED),
                            );
                            for plugin in &runtime.plugins {
                                if !search.is_empty()
                                    && !plugin.name.to_lowercase().contains(&search)
                                    && !plugin.vendor.to_lowercase().contains(&search)
                                    && !plugin.type_id.contains(&search)
                                {
                                    continue;
                                }
                                let response = ui.add_sized(
                                    Vec2::new(ui.available_width(), 26.0),
                                    egui::Button::new(
                                        RichText::new(format!(
                                            "{}  ·  {}",
                                            plugin.name, plugin.vendor
                                        ))
                                        .color(theme::TEXT_MUTED),
                                    )
                                    .frame(false),
                                );
                                if response.clicked() {
                                    selected = Some((plugin.type_id.clone(), plugin.kind, false));
                                }
                            }
                        }
                    });
            });
        if let Some((type_id, kind, stock)) = selected {
            if stock {
                self.emit(UiIntent::InsertStockDevice {
                    track_id: state.track_id,
                    chain_id: state.chain_id,
                    type_id,
                    kind,
                    index: Some(state.index),
                });
            } else {
                self.emit(UiIntent::InsertPlugin {
                    track_id: state.track_id,
                    chain_id: state.chain_id,
                    type_id,
                    kind,
                    index: Some(state.index),
                });
            }
            self.device_browser = None;
        } else if !open {
            self.device_browser = None;
        }
        let _ = doc;
    }
}

fn kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Instrument => "INSTRUMENT",
        DeviceKind::AudioFx => "AUDIO FX",
        DeviceKind::NoteFx => "NOTE FX",
        DeviceKind::Container => "CONTAINER",
    }
}
