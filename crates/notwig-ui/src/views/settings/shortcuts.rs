use egui::{Color32, RichText, TextEdit};
use notwig_ext_api::CommandDescriptor;

use crate::shell::NotwigUi;
use crate::theme;

use crate::commands::KeyChord;

enum Captured {
    Chord(KeyChord),
    Cancel,
    Pending,
}

impl NotwigUi {
    pub(crate) fn capture_shortcut(&mut self, ctx: &egui::Context) {
        let Some(command) = self.shortcut_editor.capture.clone() else {
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
                self.shortcuts.assign_id(&command, chord);
                self.shortcut_editor.capture = None;
            }
            Captured::Cancel => self.shortcut_editor.capture = None,
            Captured::Pending => {}
        }
    }

    pub(crate) fn shortcut_settings_page(&mut self, ui: &mut egui::Ui) {
        ui.label(theme::page_heading(ui.ctx(), "Shortcuts"));
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
                let extension_commands = self.commands.extension_commands();
                self.shortcuts.add_defaults(&extension_commands);
            }
        });
        ui.add_space(8.0);
        ui.separator();

        let filter = self.shortcut_editor.filter.trim().to_lowercase();
        let commands: Vec<CommandDescriptor> = self
            .commands
            .sorted()
            .into_iter()
            .filter(|command| {
                filter.is_empty()
                    || command.title.to_lowercase().contains(&filter)
                    || command.id.to_lowercase().contains(&filter)
                    || command.category.to_lowercase().contains(&filter)
                    || command.description.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("shortcut-list")
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if commands.is_empty() {
                    ui.label(
                        RichText::new("No commands match this search.").color(theme::TEXT_MUTED),
                    );
                    return;
                }
                let mut current_category = None;
                for command in &commands {
                    if current_category.as_deref() != Some(command.category.as_str()) {
                        current_category = Some(command.category.clone());
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(&command.category)
                                .font(theme::custom_font(ui.ctx(), "Geist SemiBold", 13.0))
                                .color(theme::TEXT_PRIMARY),
                        );
                    }
                    let width = ui.available_width();
                    ui.horizontal(|ui| {
                        ui.set_min_width(width * 0.36);
                        ui.label(&command.title);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let chords = self.shortcuts.bindings_for(&command.id).to_vec();
                            for chord in chords {
                                if shortcut_chip(ui, chord) {
                                    self.shortcuts.remove_id(&command.id, chord);
                                }
                            }
                            if self.shortcut_editor.capture.as_deref() == Some(&command.id) {
                                ui.label(
                                    RichText::new("Press keys…").size(12.0).color(theme::ACCENT),
                                );
                                if ui.small_button("Cancel").clicked() {
                                    self.shortcut_editor.capture = None;
                                }
                            } else if ui.small_button("+").on_hover_text("Add shortcut").clicked() {
                                self.shortcut_editor.capture = Some(command.id.clone());
                            }
                        });
                    });
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
