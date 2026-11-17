use egui::{RichText, Vec2};
use notwig_core::{DeviceKind, ProjectDoc, TrackKind};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

impl NotwigUi {
    pub(crate) fn browser_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        egui::SidePanel::left("notwig_browser")
            .resizable(true)
            .default_width(240.0)
            .min_width(180.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Browser");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            self.show_browser = false;
                        }
                    });
                });
                ui.separator();
                self.browser_content(ui, doc, runtime);
            });
    }

    pub(crate) fn browser_central(
        &mut self,
        ui: &mut egui::Ui,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        ui.add_space(8.0);
        ui.heading("Browser");
        ui.separator();
        self.browser_content(ui, doc, runtime);
    }

    fn browser_content(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, runtime: &RuntimeView) {
        ui.label(
            RichText::new("Tracks")
                .small()
                .strong()
                .color(theme::TEXT_MUTED),
        );
        egui::Grid::new("browser_tracks")
            .num_columns(2)
            .spacing(Vec2::new(4.0, 4.0))
            .show(ui, |ui| {
                if ui.button("+ Audio").clicked() {
                    self.emit(UiIntent::CreateTrack(TrackKind::Audio));
                }
                if ui.button("+ Instrument").clicked() {
                    self.emit(UiIntent::CreateTrack(TrackKind::Instrument));
                }
                ui.end_row();
                if ui.button("+ Group").clicked() {
                    self.emit(UiIntent::CreateTrack(TrackKind::Group));
                }
                if ui.button("+ FX Bus").clicked() {
                    self.emit(UiIntent::CreateTrack(TrackKind::BusFx));
                }
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.separator();

        let selected_track = self.selected_track(doc);
        let instrument_track = selected_track
            .and_then(|id| doc.tracks.get(&id))
            .is_some_and(|track| track.kind == TrackKind::Instrument);
        let can_insert = selected_track
            .and_then(|id| doc.tracks.get(&id))
            .is_some_and(|track| track.kind != TrackKind::Master);

        egui::CollapsingHeader::new("Instruments")
            .default_open(true)
            .show(ui, |ui| {
                if !instrument_track {
                    ui.label(
                        RichText::new("Select an Instrument track to insert an instrument.")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                }
                let mut any = false;
                for plugin in runtime.plugins.iter().filter(|plugin| plugin.is_instrument) {
                    any = true;
                    ui.horizontal(|ui| {
                        ui.label(&plugin.name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(instrument_track, egui::Button::new("Insert"))
                                .clicked()
                            {
                                self.insert_plugin_intent(
                                    doc,
                                    plugin.type_id.clone(),
                                    DeviceKind::Instrument,
                                );
                            }
                        });
                    });
                    ui.label(
                        RichText::new(&plugin.vendor)
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                }
                if !any {
                    ui.label(
                        RichText::new(
                            "No hosted instruments discovered. v0 ships no stock instrument.",
                        )
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                }
            });

        egui::CollapsingHeader::new("Audio Effects")
            .default_open(true)
            .show(ui, |ui| {
                let stock = [
                    ("Tool", notwig_dsp::stock::TOOL_TYPE_ID, DeviceKind::AudioFx),
                    ("EQ", notwig_dsp::stock::EQ_TYPE_ID, DeviceKind::AudioFx),
                    (
                        "Chain",
                        notwig_dsp::stock::CHAIN_TYPE_ID,
                        DeviceKind::Container,
                    ),
                ];
                for (name, type_id, kind) in stock {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(can_insert, egui::Button::new("Insert"))
                                .clicked()
                            {
                                self.insert_stock_intent(doc, type_id, kind);
                            }
                        });
                    });
                }
                for plugin in runtime
                    .plugins
                    .iter()
                    .filter(|plugin| !plugin.is_instrument)
                {
                    ui.horizontal(|ui| {
                        ui.label(&plugin.name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(can_insert, egui::Button::new("Insert"))
                                .clicked()
                            {
                                self.insert_plugin_intent(doc, plugin.type_id.clone(), plugin.kind);
                            }
                        });
                    });
                    ui.label(
                        RichText::new(&plugin.vendor)
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                }
            });

        egui::CollapsingHeader::new("Project Assets")
            .default_open(false)
            .show(ui, |ui| {
                if ui.button("Import WAV/AIFF…").clicked() {
                    self.emit(UiIntent::ImportAudio);
                }
                let audio_track = selected_track
                    .and_then(|id| doc.tracks.get(&id))
                    .filter(|track| track.kind == TrackKind::Audio)
                    .map(|track| track.id);
                if doc.assets.assets.is_empty() {
                    ui.label(
                        RichText::new(
                            "No assets. Audio import is available through the project service.",
                        )
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                }
                for asset in doc.assets.assets.values() {
                    let missing = runtime
                        .bundle_path
                        .as_ref()
                        .is_some_and(|bundle| !bundle.join(&asset.uri).exists());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                asset.format,
                                asset_kind_label(asset.kind)
                            ))
                            .small()
                            .color(if missing {
                                theme::PLAYHEAD
                            } else {
                                theme::TEXT_MUTED
                            }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if missing {
                                if ui.small_button("Relink…").clicked() {
                                    self.emit(UiIntent::RelinkAsset(asset.id));
                                }
                            } else if let Some(track_id) = audio_track {
                                if asset.kind == notwig_core::AssetKind::Audio
                                    && ui.small_button("Add clip").clicked()
                                {
                                    self.emit(UiIntent::InsertAudioClip {
                                        track_id,
                                        asset_id: asset.id,
                                    });
                                }
                            }
                        });
                    });
                }
            });
    }

    fn insert_stock_intent(&mut self, doc: &ProjectDoc, type_id: &str, kind: DeviceKind) {
        if let Some((track_id, chain_id)) = self.insert_target(doc) {
            self.emit(UiIntent::InsertStockDevice {
                track_id,
                chain_id,
                type_id: type_id.into(),
                kind,
            });
        }
    }

    fn insert_plugin_intent(&mut self, doc: &ProjectDoc, type_id: String, kind: DeviceKind) {
        if let Some((track_id, chain_id)) = self.insert_target(doc) {
            self.emit(UiIntent::InsertPlugin {
                track_id,
                chain_id,
                type_id,
                kind,
            });
        }
    }

    fn insert_target(
        &self,
        doc: &ProjectDoc,
    ) -> Option<(notwig_core::TrackId, notwig_core::DeviceChainId)> {
        let track_id = self.selected_track(doc)?;
        let track = doc.tracks.get(&track_id)?;
        Some((track_id, track.main_chain))
    }
}

fn asset_kind_label(kind: notwig_core::AssetKind) -> &'static str {
    match kind {
        notwig_core::AssetKind::Audio => "AUDIO",
        notwig_core::AssetKind::Midi => "MIDI",
        notwig_core::AssetKind::Preset => "PRESET",
        notwig_core::AssetKind::PluginState => "STATE",
        notwig_core::AssetKind::Other => "OTHER",
    }
}
