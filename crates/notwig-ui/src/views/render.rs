use egui::{Align2, RichText};

use notwig_core::{ProjectDoc, TimeMus};

use crate::form;
use crate::intent::{ExportBitDepth, ExportFormat, UiIntent};
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::{beats_to_time, format_beats};

impl NotwigUi {
    pub(crate) fn open_render(&mut self, doc: &ProjectDoc) {
        self.render_open = true;
        self.render_start = TimeMus::zero();
        self.render_end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end)
            .max()
            .unwrap_or_else(|| TimeMus::new(4, 1));
    }

    pub(crate) fn render_window(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.render_open = false;
            return;
        }
        let mut open = self.render_open;
        egui::Window::new("Render Mixdown")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([460.0, 420.0])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    form::section_heading(ui, "Format");
                    let mut format = match self.render_options.format {
                        ExportFormat::Wav => "wav",
                        ExportFormat::Aiff => "aiff",
                        ExportFormat::Flac => "flac",
                        ExportFormat::Mp3 => "mp3",
                        ExportFormat::Ogg => "ogg",
                    }
                    .to_string();
                    let format_label = match self.render_options.format {
                        ExportFormat::Wav => "WAV",
                        ExportFormat::Aiff => "AIFF",
                        ExportFormat::Flac => "FLAC",
                        ExportFormat::Mp3 => "MP3",
                        ExportFormat::Ogg => "OGG Vorbis",
                    };
                    if form::select_row(
                        ui,
                        "File format",
                        "render-format",
                        &mut format,
                        format_label,
                        vec![
                            ("wav".to_string(), "WAV".to_string()),
                            ("aiff".to_string(), "AIFF".to_string()),
                            ("flac".to_string(), "FLAC".to_string()),
                            ("mp3".to_string(), "MP3".to_string()),
                            ("ogg".to_string(), "OGG Vorbis".to_string()),
                        ],
                    ) {
                        self.render_options.format = match format.as_str() {
                            "aiff" => ExportFormat::Aiff,
                            "flac" => ExportFormat::Flac,
                            "mp3" => ExportFormat::Mp3,
                            "ogg" => ExportFormat::Ogg,
                            _ => ExportFormat::Wav,
                        };
                        if self.render_options.format == ExportFormat::Flac
                            && self.render_options.bit_depth == ExportBitDepth::Float32
                        {
                            self.render_options.bit_depth = ExportBitDepth::Int24;
                        }
                    }

                    let depth_options = match self.render_options.format {
                        ExportFormat::Wav => Some(vec![
                            ("int16".to_string(), "16-bit PCM".to_string()),
                            ("int24".to_string(), "24-bit PCM".to_string()),
                            ("float32".to_string(), "32-bit float".to_string()),
                        ]),
                        ExportFormat::Aiff => Some(vec![
                            ("int16".to_string(), "16-bit PCM".to_string()),
                            ("int24".to_string(), "24-bit PCM".to_string()),
                            ("float32".to_string(), "32-bit PCM".to_string()),
                        ]),
                        ExportFormat::Flac => Some(vec![
                            ("int16".to_string(), "16-bit PCM".to_string()),
                            ("int24".to_string(), "24-bit PCM".to_string()),
                        ]),
                        ExportFormat::Mp3 | ExportFormat::Ogg => None,
                    };
                    if let Some(depth_options) = depth_options {
                        let mut depth = match self.render_options.bit_depth {
                            ExportBitDepth::Int16 => "int16".to_string(),
                            ExportBitDepth::Int24 => "int24".to_string(),
                            ExportBitDepth::Float32 => "float32".to_string(),
                        };
                        let depth_label = match self.render_options.bit_depth {
                            ExportBitDepth::Int16 => "16-bit PCM",
                            ExportBitDepth::Int24 => "24-bit PCM",
                            ExportBitDepth::Float32 => "32-bit",
                        };
                        if form::select_row(
                            ui,
                            "Bit depth",
                            "render-depth",
                            &mut depth,
                            depth_label,
                            depth_options,
                        ) {
                            self.render_options.bit_depth = match depth.as_str() {
                                "int16" => ExportBitDepth::Int16,
                                "int24" => ExportBitDepth::Int24,
                                _ => ExportBitDepth::Float32,
                            };
                        }
                    }

                    ui.checkbox(&mut self.render_options.dither, "Dither");
                    ui.checkbox(&mut self.render_options.normalize, "Normalize");
                    ui.checkbox(&mut self.render_options.stems, "Export stems");

                    ui.add_space(10.0);
                    form::section_heading(ui, "Range");
                    ui.horizontal(|ui| {
                        let mut start = self.render_start.as_f64();
                        if ui
                            .add(
                                egui::DragValue::new(&mut start)
                                    .speed(0.01)
                                    .suffix(" beats"),
                            )
                            .changed()
                        {
                            self.render_start = beats_to_time(start.max(0.0));
                        }
                        ui.label("to");
                        let mut end = self.render_end.as_f64();
                        if ui
                            .add(egui::DragValue::new(&mut end).speed(0.01).suffix(" beats"))
                            .changed()
                        {
                            self.render_end = beats_to_time(end.max(0.0));
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} → {}",
                            format_beats(
                                self.render_start,
                                doc.meta.bar_length(),
                                doc.meta.beat_length()
                            ),
                            format_beats(
                                self.render_end,
                                doc.meta.bar_length(),
                                doc.meta.beat_length()
                            )
                        ))
                        .monospace()
                        .color(theme::TEXT_MUTED),
                    );
                    if let Some(range) = doc.transport.loop_range
                        && ui.button("Copy loop range").clicked()
                    {
                        self.render_start = range.start;
                        self.render_end = range.end;
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        let valid = self.render_end > self.render_start;
                        if ui
                            .add_enabled(valid, egui::Button::new("Render…"))
                            .clicked()
                        {
                            let end = self.render_end.max(self.render_start + TimeMus::new(1, 16));
                            self.emit(UiIntent::RenderMixdownRange {
                                start: self.render_start,
                                end,
                                options: self.render_options,
                            });
                        }
                        if ui.button("Close").clicked() {
                            self.render_open = false;
                        }
                    });
                });
            });
        self.render_open = open;
    }
}
