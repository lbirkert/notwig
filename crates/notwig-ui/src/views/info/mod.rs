mod audio_clip;
mod common;
mod device;
mod files;
mod midi_clip;
mod time;
mod track;

use egui::{Color32, RichText, Vec2};
use notwig_core::{ClipEntity, ClipId, ClipPayload, ProjectDoc, RangeMus, TimeMus};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::{beats_to_time, draggable_number, format_beats};

use common::color_swatch_row;

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
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} clips selected"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
        self.clip_header(ui, doc, &clip);
        self.clip_color_editor(ui, &clip);
        match &clip.payload {
            ClipPayload::Midi(midi) => self.midi_clip_info(ui, midi),
            ClipPayload::Audio(audio) => self.audio_clip_info(ui, doc, audio),
        }
        self.clip_actions(ui, clip_id);
    }

    fn clip_color_editor(&mut self, ui: &mut egui::Ui, clip: &ClipEntity) {
        ui.add_space(4.0);
        if let Some(color) = color_swatch_row(ui, clip.color) {
            self.emit(UiIntent::SetClipColor {
                clip_id: clip.id,
                color,
            });
        }
    }

    fn clip_actions(&mut self, ui: &mut egui::Ui, clip_id: ClipId) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                self.emit(UiIntent::DuplicateClip { clip_id });
            }
            if ui.button("Delete").clicked() {
                self.emit(UiIntent::DeleteClip(clip_id));
            }
        });
    }

    fn clip_header(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, clip: &ClipEntity) {
        ui.horizontal(|ui| {
            let title = match &clip.payload {
                ClipPayload::Midi(_) => "MIDI CLIP",
                ClipPayload::Audio(_) => "AUDIO CLIP",
            };
            ui.label(theme::heading(ui.ctx(), title).color(if clip.muted {
                Color32::from_rgb(104, 108, 120)
            } else {
                theme::HEADING
            }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::icon_button_sized(
                    ui,
                    crate::icons::Icon::Power,
                    Vec2::new(26.0, 22.0),
                    !clip.muted,
                    if clip.muted {
                        "Activate clip"
                    } else {
                        "Deactivate clip"
                    },
                )
                .clicked()
                {
                    self.emit(UiIntent::SetClipMuted {
                        clip_id: clip.id,
                        muted: !clip.muted,
                    });
                }
            });
        });
        let mut name = clip.name.clone().unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Clip name")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.emit(UiIntent::RenameClip {
                clip_id: clip.id,
                name,
            });
        }
        self.clip_range_fields(ui, doc, clip);
    }

    fn clip_range_fields(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, clip: &ClipEntity) {
        let bar_length = doc.meta.bar_length();
        let beat_length = doc.meta.beat_length();
        let format = |beats: f64| format_beats(beats_to_time(beats), bar_length, beat_length);
        let min_length = TimeMus::new(1, 16);
        let min_length_beats = min_length.as_f64();

        let start = clip.range.start.as_f64();
        let end = clip.range.end.as_f64();
        let length = clip.range.length().as_f64();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Start").small().color(theme::TEXT_MUTED));
            if let Some(value) = draggable_number(
                ui,
                ui.id().with(("clip-start", clip.id)),
                start,
                0.0,
                (end - min_length_beats).max(0.0),
                start,
                0.01,
                format,
            ) {
                let end_time = clip.range.end;
                let mut start_time = beats_to_time(value).max(TimeMus::zero());
                if end_time - start_time < min_length {
                    start_time = (end_time - min_length).max(TimeMus::zero());
                }
                if let Ok(range) = RangeMus::new(start_time, end_time) {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: clip.id,
                        range,
                    });
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("End").small().color(theme::TEXT_MUTED));
            if let Some(value) = draggable_number(
                ui,
                ui.id().with(("clip-end", clip.id)),
                end,
                start + min_length_beats,
                f64::INFINITY,
                end,
                0.01,
                format,
            ) {
                let start_time = clip.range.start;
                let mut end_time = beats_to_time(value);
                let min_end = start_time + min_length;
                if end_time < min_end {
                    end_time = min_end;
                }
                if let Ok(range) = RangeMus::new(start_time, end_time) {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: clip.id,
                        range,
                    });
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("Length").small().color(theme::TEXT_MUTED));
            if let Some(value) = draggable_number(
                ui,
                ui.id().with(("clip-length", clip.id)),
                length,
                min_length_beats,
                f64::INFINITY,
                length,
                0.01,
                format,
            ) {
                let start_time = clip.range.start;
                let mut length_time = beats_to_time(value);
                if length_time < min_length {
                    length_time = min_length;
                }
                if let Ok(range) = RangeMus::new(start_time, start_time + length_time) {
                    self.emit(UiIntent::SetClipRange {
                        clip_id: clip.id,
                        range,
                    });
                }
            }
        });
    }
}
