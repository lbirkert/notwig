use egui::{Align2, RichText};

use notwig_core::{ProjectDoc, TimeMus};

use crate::form;
use crate::intent::UiIntent;
use crate::shell::NotwigUi;
use crate::theme;
use crate::widgets::format_beats;

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
            .default_size([460.0, 300.0])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    form::section_heading(ui, "Format");
                    ui.label(RichText::new("WAV · 32-bit float").color(theme::TEXT_MUTED));
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
                            self.render_start = TimeMus::new(
                                (start.max(0.0) * 1_000_000.0).round() as i64,
                                1_000_000,
                            );
                        }
                        ui.label("to");
                        let mut end = self.render_end.as_f64();
                        if ui
                            .add(egui::DragValue::new(&mut end).speed(0.01).suffix(" beats"))
                            .changed()
                        {
                            self.render_end = TimeMus::new(
                                (end.max(0.0) * 1_000_000.0).round() as i64,
                                1_000_000,
                            );
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} → {}",
                            format_beats(self.render_start, doc.meta.time_sig_num),
                            format_beats(self.render_end, doc.meta.time_sig_num)
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
