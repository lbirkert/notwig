use egui::{Color32, RichText, TextEdit};

use crate::shell::NotwigUi;
use crate::theme;

use crate::commands::{COMMANDS, CommandCategory, KeyChord};

enum Captured {
    Chord(KeyChord),
    Cancel,
    Pending,
}

impl NotwigUi {
    pub(crate) fn capture_shortcut(&mut self, ctx: &egui::Context) {
        let Some(command) = self.shortcut_editor.capture else {
            return;
        };
        let captured = ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                return Captured::Cancel;
            }
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                {
                    if *key == egui::Key::Escape {
                        return Captured::Cancel;
                    }
                    return Captured::Chord(KeyChord::from_modifiers(*key, *modifiers));
                }
            }
            Captured::Pending
        });
        match captured {
            Captured::Chord(chord) => {
                self.shortcuts.assign(command, chord);
                self.shortcut_editor.capture = None;
            }
            Captured::Cancel => self.shortcut_editor.capture = None,
            Captured::Pending => {}
        }
    }

    pub(crate) fn shortcut_settings_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Shortcuts");
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Click + and press a key combination. A command can have multiple shortcuts.",
            )
            .color(theme::TEXT_MUTED),
        );
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.shortcut_editor.filter)
                    .hint_text("Search commands")
                    .desired_width(360.0),
            );
            if ui.button("Reset Defaults").clicked() {
                self.shortcuts.reset();
            }
        });
        ui.add_space(8.0);
        ui.separator();

        let filter = self.shortcut_editor.filter.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("shortcut-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut any = false;
                for category in CommandCategory::ALL {
                    let commands: Vec<_> = COMMANDS
                        .iter()
                        .filter(|spec| {
                            spec.category == category
                                && (filter.is_empty()
                                    || spec.label.to_lowercase().contains(&filter)
                                    || spec.id.to_lowercase().contains(&filter)
                                    || category.label().to_lowercase().contains(&filter))
                        })
                        .collect();
                    if commands.is_empty() {
                        continue;
                    }
                    any = true;
                    egui::CollapsingHeader::new(category.label())
                        .id_salt(("shortcut-category", category.label()))
                        .default_open(true)
                        .show(ui, |ui| {
                            let width = ui.available_width();
                            egui::Grid::new(("shortcut-grid", category.label()))
                                .num_columns(2)
                                .min_col_width(width * 0.38)
                                .spacing([18.0, 6.0])
                                .show(ui, |ui| {
                                    for spec in commands {
                                        ui.label(spec.label);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            let chords =
                                                self.shortcuts.bindings(spec.command).to_vec();
                                            for chord in chords {
                                                if shortcut_chip(ui, chord) {
                                                    self.shortcuts.remove(spec.command, chord);
                                                }
                                            }
                                            if self.shortcut_editor.capture == Some(spec.command) {
                                                ui.label(
                                                    RichText::new("Press keys…")
                                                        .size(12.0)
                                                        .color(theme::ACCENT),
                                                );
                                                if ui.small_button("Cancel").clicked() {
                                                    self.shortcut_editor.capture = None;
                                                }
                                            } else if ui
                                                .small_button("+")
                                                .on_hover_text("Add shortcut")
                                                .clicked()
                                            {
                                                self.shortcut_editor.capture = Some(spec.command);
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                        });
                }
                if !any {
                    ui.label(
                        RichText::new("No commands match this search.").color(theme::TEXT_MUTED),
                    );
                }
            });
    }
}

fn shortcut_chip(ui: &mut egui::Ui, chord: KeyChord) -> bool {
    let mut remove = false;
    egui::Frame::new()
        .fill(Color32::from_rgb(8, 9, 12))
        .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(RichText::new(chord.label()).monospace().size(11.0));
                if ui
                    .add(egui::Button::new(RichText::new("×").size(10.0)).frame(false))
                    .on_hover_text("Remove shortcut")
                    .clicked()
                {
                    remove = true;
                }
            });
        });
    remove
}
