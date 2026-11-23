use egui::RichText;

use crate::commands::{COMMANDS, CommandCategory, ShortcutCommand, Shortcuts};
use crate::theme;

use super::NotwigUi;

impl NotwigUi {
    pub(super) fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        let shortcuts = &self.shortcuts;
        egui::Window::new("Keyboard & workflow")
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Command shortcuts reflect your current bindings; customize them in \
                         Settings -> Shortcuts.",
                    )
                    .color(theme::TEXT_MUTED),
                );
                ui.add_space(6.0);
                for category in CommandCategory::ALL {
                    ui.label(
                        RichText::new(category.label())
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    for spec in COMMANDS.iter().filter(|spec| spec.category == category) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(binding_label(shortcuts, spec.command))
                                    .monospace()
                                    .color(theme::ACCENT),
                            );
                            ui.label(RichText::new(spec.label).strong());
                            ui.label(RichText::new(spec.description).color(theme::TEXT_MUTED));
                        });
                    }
                    ui.add_space(4.0);
                }
                ui.separator();
                ui.label(
                    RichText::new("Mouse & workflow")
                        .strong()
                        .color(theme::TEXT_MUTED),
                );
                for (key, action) in [
                    ("Alt while dragging", "Bypass 1/16 snapping"),
                    ("Double-click empty lane", "Create 4-beat MIDI clip"),
                    ("Drag clip body / edges", "Move or trim clip"),
                    ("Right-click clip", "Mute, duplicate, delete"),
                    (
                        "Click ruler",
                        "Set play start (does not move the playhead while playing)",
                    ),
                    (
                        "Drag lower half of a lane",
                        "Select a time range across lanes (Shift bypasses snap)",
                    ),
                    ("Shift+drag ruler", "Set loop range"),
                    ("Middle-drag", "Pan timeline horizontally / zoom vertically"),
                    ("Pinch / two-finger swipe", "Zoom / pan the whole timeline"),
                    ("Drag audio file", "Import and place it on an audio track"),
                    (
                        "EQ band number",
                        "Drag gain/frequency; handle adjusts Q/frequency",
                    ),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).monospace().color(theme::ACCENT));
                        ui.label(action);
                    });
                }
            });
        self.show_help = open;
    }
}

fn binding_label(shortcuts: &Shortcuts, command: ShortcutCommand) -> String {
    let bindings = shortcuts.bindings(command);
    if bindings.is_empty() {
        "Unassigned".to_owned()
    } else {
        bindings
            .iter()
            .map(|chord| chord.label())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}
