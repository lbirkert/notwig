mod audio_clip;
mod common;
mod device;
mod files;
mod midi_clip;
mod time;
mod track;

use egui::{RichText, Vec2};
use notwig_core::{ClipEntity, ClipId, ClipPayload, ProjectDoc};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_beats;

use common::section_header;

impl NotwigUi {
    pub(crate) fn info_panel(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        if !self.show_info {
            return;
        }
        egui::SidePanel::left("notwig_info")
            .resizable(false)
            .exact_width(250.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 0,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ctx, |ui| {
                let height = ui.available_height();
                let handle_width = 10.0;
                // Leave room for the collapse handle *and* the horizontal item
                // spacing; otherwise the handle overflows the panel and the
                // central panel paints a dark strip over its right edge.
                let content_width =
                    (ui.available_width() - handle_width - ui.spacing().item_spacing.x).max(120.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let track_only = self.state.selection.primary_device().is_none()
                                && self.state.selection.primary_clip().is_none()
                                && self.state.selection.time.is_none();
                            if track_only
                                && let Some(track_id) = self.state.selection.primary_track()
                            {
                                self.track_info(ui, doc, runtime, track_id);
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("info-content")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        if let Some(device_id) =
                                            self.state.selection.primary_device()
                                        {
                                            self.device_info(ui, doc, device_id);
                                        } else if let Some(clip_id) =
                                            self.state.selection.primary_clip()
                                        {
                                            self.clip_info(ui, doc, clip_id);
                                        } else if let Some(time) = self.state.selection.time.clone()
                                        {
                                            self.time_info(ui, doc, &time);
                                        } else {
                                            ui.label(
                                                RichText::new(
                                                    "No selection. Click a track, clip, or device.",
                                                )
                                                .color(theme::TEXT_MUTED),
                                            );
                                        }
                                    });
                            }
                        },
                    );
                    if crate::widgets::collapse_handle(
                        ui,
                        Vec2::new(handle_width, height),
                        true,
                        "info-panel",
                        "Drag left to collapse",
                    ) {
                        self.show_info = false;
                    }
                });
            });
    }

    fn clip_info(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, clip_id: ClipId) {
        let Some(clip) = doc.arranger.clips.get(&clip_id).cloned() else {
            return;
        };
        let count = self.state.selection.clips.len();
        section_header(ui, if count > 1 { "CLIPS" } else { "CLIP" });
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} clips selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.separator();
        }
        clip_header(ui, doc, &clip);
        match &clip.payload {
            ClipPayload::Midi(midi) => self.midi_clip_info(ui, midi),
            ClipPayload::Audio(audio) => self.audio_clip_info(ui, doc, audio),
        }
        self.clip_actions(ui, clip_id, clip.muted);
    }

    fn clip_actions(&mut self, ui: &mut egui::Ui, clip_id: ClipId, muted: bool) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut muted = muted;
            if ui.toggle_value(&mut muted, "Muted").changed() {
                self.emit(UiIntent::SetClipMuted { clip_id, muted });
            }
            if ui.button("Duplicate").clicked() {
                self.emit(UiIntent::DuplicateClip { clip_id });
            }
            if ui.button("Delete").clicked() {
                self.emit(UiIntent::DeleteClip(clip_id));
            }
        });
    }
}

fn clip_header(ui: &mut egui::Ui, doc: &ProjectDoc, clip: &ClipEntity) {
    let name = clip.name.clone().unwrap_or_else(|| "Clip".into());
    ui.label(RichText::new(name).strong().size(15.0));
    ui.label(
        RichText::new(match &clip.payload {
            ClipPayload::Midi(_) => "MIDI CLIP",
            ClipPayload::Audio(_) => "AUDIO CLIP",
        })
        .small()
        .monospace()
        .color(theme::TEXT_MUTED),
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new("Track").small().color(theme::TEXT_MUTED));
        ui.label(
            doc.tracks
                .get(&clip.track_id)
                .map_or("Unknown", |track| track.name.as_str()),
        );
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Start").small().color(theme::TEXT_MUTED));
        ui.label(format_beats(clip.range.start, doc.meta.time_sig_num));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Length").small().color(theme::TEXT_MUTED));
        ui.label(format_beats(clip.range.length(), doc.meta.time_sig_num));
    });
}
