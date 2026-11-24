use egui::Align2;

use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

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
            .default_size([1120.0, 700.0])
            .min_size([900.0, 500.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::APP_BAR)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID_STRONG))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                ui.set_min_width(1120.0);
                ui.set_min_height(640.0);
                egui::SidePanel::left("settings-sidebar")
                    .resizable(false)
                    .exact_width(236.0)
                    .show_separator_line(true)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::APP_BAR)
                            .inner_margin(egui::Margin {
                                left: 8,
                                right: 8,
                                top: 10,
                                bottom: 10,
                            }),
                    )
                    .show_inside(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for category in [
                            SettingsCategory::Shortcuts,
                            SettingsCategory::Extensions,
                            SettingsCategory::Audio,
                        ] {
                            let selected = self.settings_category == category;
                            if category_button(ui, category.label(), selected) {
                                self.settings_category = category;
                            }
                        }
                    });
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(theme::PANEL_LIGHT)
                            .inner_margin(egui::Margin {
                                left: 18,
                                right: 12,
                                top: 12,
                                bottom: 12,
                            }),
                    )
                    .show_inside(ui, |ui| match self.settings_category {
                        SettingsCategory::Shortcuts => {
                            self.shortcut_settings_page(ui);
                        }
                        SettingsCategory::Extensions => {
                            self.extensions_settings_page(ui, runtime);
                        }
                        SettingsCategory::Audio => {
                            egui::ScrollArea::vertical()
                                .id_salt("settings-content")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.audio_settings_page(ui, runtime);
                                    ui.add_space(20.0);
                                });
                        }
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

fn category_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
    let background = if selected {
        Some(theme::ACCENT_DIM)
    } else if response.hovered() {
        Some(theme::PANEL_LIGHT)
    } else {
        None
    };
    if let Some(background) = background {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(5), background);
    }
    let text_color = if selected || response.hovered() {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_MUTED
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        theme::custom_font(ui.ctx(), "Geist Medium", 13.0),
        text_color,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

#[doc(hidden)]
pub mod test_api {
    pub use super::extensions::{
        extension_target_labels, extension_target_supported, extension_update_available,
    };
}
