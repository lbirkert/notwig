use egui::RichText;
use notwig_core::ProjectDoc;

use crate::shell::NotwigUi;
use crate::state::TimeSelection;
use crate::theme;
use crate::widgets::format_beats;

use super::common::section_header;

impl NotwigUi {
    #[allow(clippy::unused_self)]
    pub(super) fn time_info(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, time: &TimeSelection) {
        section_header(ui, "TIME SELECTION");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Start").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(
                time.range.start,
                doc.meta.bar_length(),
                doc.meta.beat_length(),
            ));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("End").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(
                time.range.end,
                doc.meta.bar_length(),
                doc.meta.beat_length(),
            ));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Length").small().color(theme::TEXT_MUTED));
            ui.label(format_beats(
                time.range.length(),
                doc.meta.bar_length(),
                doc.meta.beat_length(),
            ));
        });
        ui.separator();
        ui.label(
            RichText::new(format!("{} lane(s)", time.tracks.len()))
                .small()
                .color(theme::TEXT_MUTED),
        );
        let selected: Vec<_> = time.tracks.iter().copied().collect();
        for id in selected {
            if let Some(track) = doc.tracks.get(&id) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&track.name).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(track.kind.name().to_uppercase())
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                    });
                });
            }
        }
    }
}
