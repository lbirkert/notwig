use egui::RichText;

use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;

mod audio;
mod audio_channels;
mod extensions;
mod shortcuts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    Shortcuts,
    Extensions,
    #[default]
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExtensionFilter {
    #[default]
    All,
    Available,
    Installed,
}

impl SettingsCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Shortcuts => "Shortcuts",
            Self::Extensions => "Extensions",
            Self::Audio => "Audio",
        }
    }
}

impl NotwigUi {
    pub(crate) fn open_settings(&mut self, runtime: &RuntimeView) {
        self.settings_open = true;
        self.settings_category = SettingsCategory::Audio;
        self.audio_draft = runtime.audio_settings.current.clone();
        self.shortcut_editor.capture = None;
    }

    pub(crate) fn settings_window(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        if self.shortcut_editor.capture.is_some() {
            self.capture_shortcut(ctx);
        } else if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
            return;
        }
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([1000.0, 620.0])
            .min_size([760.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(190.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(8.0);
                            for category in [
                                SettingsCategory::Shortcuts,
                                SettingsCategory::Extensions,
                                SettingsCategory::Audio,
                            ] {
                                let selected = self.settings_category == category;
                                if ui
                                    .selectable_label(
                                        selected,
                                        RichText::new(category.label()).size(14.0),
                                    )
                                    .clicked()
                                {
                                    self.settings_category = category;
                                }
                            }
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.add_space(14.0);
                                if ui.button("Close").clicked() {
                                    self.settings_open = false;
                                }
                            });
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| match self.settings_category {
                            SettingsCategory::Shortcuts => {
                                ui.add_space(10.0);
                                self.shortcut_settings_page(ui);
                            }
                            SettingsCategory::Extensions => {
                                ui.add_space(10.0);
                                self.extensions_settings_page(ui, runtime);
                            }
                            SettingsCategory::Audio => {
                                egui::ScrollArea::vertical()
                                    .id_salt("settings-content")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.add_space(10.0);
                                        self.audio_settings_page(ui, runtime);
                                        ui.add_space(20.0);
                                    });
                            }
                        },
                    );
                });
            });
        self.settings_open = open;
        if !open {
            self.shortcut_editor.capture = None;
            self.show_extension_index_manager = false;
        } else if self.show_extension_index_manager {
            self.extension_index_manager_window(ctx, runtime);
        }
    }
}

#[doc(hidden)]
pub mod test_api {
    pub use super::extensions::{
        extension_target_labels, extension_target_supported, extension_update_available,
    };
}
