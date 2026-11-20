use egui::{Color32, RichText};

use crate::form;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

mod shortcuts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    Shortcuts,
    Extensions,
    #[default]
    Audio,
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
                                egui::ScrollArea::vertical()
                                    .id_salt("extensions-content")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.add_space(10.0);
                                        self.extensions_settings_page(ui, runtime);
                                        ui.add_space(20.0);
                                    });
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
        }
    }

    fn extensions_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        ui.heading("Extensions");
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Install and manage WASM extensions from index sources or local ZIP files.",
            )
            .small()
            .color(theme::TEXT_MUTED),
        );

        if !runtime.extension_index_loaded
            && !runtime.extension_index_loading
            && !self.extension_index_requested
            && !runtime.extension_indexes.is_empty()
        {
            self.extension_index_requested = true;
            self.emit(UiIntent::RefreshExtensionIndex);
        }

        form::section_heading(ui, "Index Sources");
        let indexes = runtime.extension_indexes.clone();
        if indexes.is_empty() {
            ui.label(
                RichText::new("No index sources configured.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else {
            for index in &indexes {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(index)
                            .small()
                            .monospace()
                            .color(theme::TEXT_MUTED),
                    );
                    if ui.small_button("Remove").clicked() {
                        let next: Vec<String> = indexes
                            .iter()
                            .filter(|other| *other != index)
                            .cloned()
                            .collect();
                        self.extension_index_requested = false;
                        self.emit(UiIntent::SetExtensionIndexes { indexes: next });
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.extension_index_input)
                    .hint_text("https://... or /path/to/extensions.json")
                    .desired_width(420.0),
            );
            if ui.button("Add index").clicked() {
                let value = self.extension_index_input.trim().to_owned();
                if !value.is_empty() && !indexes.contains(&value) {
                    let mut next = indexes.clone();
                    next.push(value);
                    self.extension_index_input.clear();
                    self.extension_index_requested = false;
                    self.emit(UiIntent::SetExtensionIndexes { indexes: next });
                }
            }
            let label = if runtime.extension_index_loading {
                "Refreshing..."
            } else {
                "Refresh"
            };
            if ui.button(label).clicked() && !runtime.extension_index_loading {
                self.emit(UiIntent::RefreshExtensionIndex);
            }
        });
        for error in &runtime.extension_index_errors {
            ui.label(RichText::new(error).small().color(theme::PLAYHEAD));
        }
        if let Some(error) = &runtime.extensions_error {
            ui.label(RichText::new(error).small().color(theme::PLAYHEAD));
        }

        form::section_heading(ui, "Import");
        ui.horizontal(|ui| {
            if ui.button("Import ZIP").clicked() {
                self.emit(UiIntent::ImportExtensionZip);
            }
            ui.label(
                RichText::new("Drop a .zip onto this page to install.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        });

        form::section_heading(ui, "Available");
        if runtime.extension_index_loading {
            ui.label(
                RichText::new("Loading extension index...")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else if runtime.extension_index.is_empty() {
            ui.label(
                RichText::new("No extensions found. Add an index source and refresh.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else {
            for entry in runtime.extension_index.clone() {
                let installed = runtime
                    .extensions
                    .iter()
                    .find(|info| info.id == entry.id)
                    .cloned();
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&entry.name).strong().size(14.0));
                            ui.label(
                                RichText::new(format!("{} {}", entry.vendor, entry.version))
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = if installed.is_some() {
                                        "Installed"
                                    } else {
                                        "Available"
                                    };
                                    ui.label(RichText::new(label).small().monospace().color(
                                        if installed.is_some() {
                                            theme::ACCENT
                                        } else {
                                            theme::TEXT_MUTED
                                        },
                                    ));
                                },
                            );
                        });
                        if !entry.capabilities.is_empty() {
                            let capabilities = entry
                                .capabilities
                                .iter()
                                .map(|capability| match capability {
                                    notwig_ext_api::ExtensionCapability::Device => "device",
                                    notwig_ext_api::ExtensionCapability::UiDraw => "ui.draw",
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(RichText::new(capabilities).small().color(theme::TEXT_MUTED));
                        }
                        if !entry.devices.is_empty() {
                            let devices = entry
                                .devices
                                .iter()
                                .map(|device| device.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(RichText::new(devices).small().color(theme::TEXT_MUTED));
                        }
                        ui.horizontal(|ui| {
                            if let Some(info) = installed.as_ref() {
                                if self.pending_extension_replace.as_deref()
                                    == Some(entry.id.as_str())
                                {
                                    ui.label(RichText::new("Replace installed extension?").small());
                                    if ui.button("Replace").clicked() {
                                        self.pending_extension_replace = None;
                                        self.emit(UiIntent::InstallExtensionFromIndex {
                                            id: entry.id.clone(),
                                        });
                                    }
                                    if ui.button("Cancel").clicked() {
                                        self.pending_extension_replace = None;
                                    }
                                } else {
                                    ui.label(
                                        RichText::new(format!("Installed {}", info.version))
                                            .small()
                                            .color(theme::TEXT_MUTED),
                                    );
                                    if ui.button("Reinstall").clicked() {
                                        self.pending_extension_replace = Some(entry.id.clone());
                                    }
                                }
                            } else if ui.button("Install").clicked() {
                                self.emit(UiIntent::InstallExtensionFromIndex {
                                    id: entry.id.clone(),
                                });
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        }

        form::section_heading(ui, "Installed");
        ui.horizontal(|ui| {
            if ui.button("Reload extensions").clicked() {
                self.emit(UiIntent::ReloadExtensions);
            }
            ui.label(
                RichText::new(format!("{} installed", runtime.extensions.len()))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        });
        if runtime.extensions.is_empty() {
            ui.label(
                RichText::new("No extensions installed.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            return;
        }
        for info in runtime.extensions.clone() {
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&info.name).strong().size(14.0));
                        ui.label(
                            RichText::new(format!("{} {}", info.vendor, info.version))
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(info.status.label())
                                    .small()
                                    .monospace()
                                    .color(match info.status {
                                        notwig_ext_api::ExtensionStatus::Loaded => theme::ACCENT,
                                        notwig_ext_api::ExtensionStatus::Failed => theme::PLAYHEAD,
                                        _ => theme::TEXT_MUTED,
                                    }),
                            );
                        });
                    });
                    ui.horizontal(|ui| {
                        let mut enabled = info.enabled;
                        if ui.checkbox(&mut enabled, "Enabled").changed() {
                            self.emit(UiIntent::SetExtensionEnabled {
                                id: info.id.clone(),
                                enabled,
                            });
                        }
                        let mut trust = info.trust;
                        egui::ComboBox::from_id_salt(("extension-trust", &info.id))
                            .selected_text(trust.label())
                            .show_ui(ui, |ui| {
                                for level in [
                                    notwig_ext_api::TrustLevel::Trusted,
                                    notwig_ext_api::TrustLevel::Limited,
                                    notwig_ext_api::TrustLevel::DrawOnly,
                                ] {
                                    ui.selectable_value(&mut trust, level, level.label());
                                }
                            });
                        if trust != info.trust {
                            self.emit(UiIntent::SetExtensionTrust {
                                id: info.id.clone(),
                                trust,
                            });
                        }
                        if info.device_count > 0 {
                            ui.label(
                                RichText::new(format!("{} devices", info.device_count))
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Uninstall").clicked() {
                                self.pending_extension_replace = None;
                                self.emit(UiIntent::UninstallExtension {
                                    id: info.id.clone(),
                                });
                            }
                        });
                    });
                    if let Some(error) = &info.error {
                        ui.label(RichText::new(error).small().color(theme::PLAYHEAD));
                    }
                });
            ui.add_space(4.0);
        }
    }

    fn audio_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        let view = &runtime.audio_settings;
        ui.heading("Audio");
        ui.add_space(4.0);
        form::section_heading(ui, "Audio System");
        ui.add_space(4.0);

        let current = &view.current;
        if self.audio_draft.driver.is_empty() {
            self.audio_draft.driver.clone_from(&current.driver);
            self.audio_draft.device.clone_from(&current.device);
            self.audio_draft
                .sample_rate
                .clone_from(&current.sample_rate);
            if self.audio_draft.block_size == 0 {
                self.audio_draft.block_size = current.block_size.max(256);
            }
        }

        let driver_label = self.driver_label(view);
        let drivers = view
            .drivers
            .iter()
            .map(|driver| (driver.id.clone(), driver.label.clone()))
            .collect();
        if form::select_row(
            ui,
            "Driver",
            "audio-driver",
            &mut self.audio_draft.driver,
            driver_label,
            drivers,
        ) {
            self.audio_draft.device = view
                .drivers
                .iter()
                .find(|driver| driver.id == self.audio_draft.driver)
                .and_then(|driver| driver.devices.first())
                .map(|device| device.id.clone())
                .unwrap_or_default();
            self.audio_draft.sample_rate = "auto".into();
        }

        let device_label = self.device_label(view);
        let devices = self
            .devices_for(view)
            .iter()
            .map(|device| (device.id.clone(), device.label.clone()))
            .collect();
        if form::select_row(
            ui,
            "Device",
            "audio-device",
            &mut self.audio_draft.device,
            device_label,
            devices,
        ) {
            self.audio_draft.sample_rate = "auto".into();
        }

        let rate_label = self.sample_rate_label(view);
        let mut rates = vec![("auto".to_string(), "Automatic".to_string())];
        rates.extend(
            self.sample_rates_for(view)
                .into_iter()
                .map(|rate| (rate.to_string(), format!("{rate} Hz"))),
        );
        form::select_row(
            ui,
            "Sample rate",
            "audio-rate",
            &mut self.audio_draft.sample_rate,
            rate_label,
            rates,
        );

        ui.horizontal(|ui| {
            ui.label(RichText::new("Block size").size(13.0));
            if self.audio_draft.block_size == 0 {
                self.audio_draft.block_size = 512;
            }
            ui.add(
                egui::Slider::new(&mut self.audio_draft.block_size, 16..=2048)
                    .step_by(16.0)
                    .show_value(false),
            );
            let rate = self
                .audio_draft
                .sample_rate
                .parse::<u32>()
                .ok()
                .or_else(|| current.sample_rate.parse::<u32>().ok())
                .unwrap_or(48_000);
            let latency_ms = f64::from(self.audio_draft.block_size) / f64::from(rate) * 1000.0;
            ui.label(
                RichText::new(format!(
                    "{} samples / {:.2} ms",
                    self.audio_draft.block_size, latency_ms
                ))
                .monospace()
                .color(theme::ACCENT),
            );
        });
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                let sample_rate = self.audio_draft.sample_rate.parse::<u32>().ok();
                let driver = if self.audio_draft.driver.is_empty() {
                    current.driver.clone()
                } else {
                    self.audio_draft.driver.clone()
                };
                let device = if self.audio_draft.device.is_empty() {
                    current.device.clone()
                } else {
                    self.audio_draft.device.clone()
                };
                self.emit(UiIntent::ApplyAudioSettings {
                    driver,
                    device,
                    sample_rate,
                    block_size: self.audio_draft.block_size,
                });
            }
            if ui.button("Revert").clicked() {
                self.audio_draft = current.clone();
            }
        });

        if let Some(error) = &view.error {
            ui.add_space(10.0);
            ui.label(RichText::new(error).color(Color32::from_rgb(240, 120, 110)));
        }
        if let Some(warning) = &view.warning {
            ui.add_space(10.0);
            ui.label(RichText::new(warning).color(theme::ACCENT));
        }
    }

    fn driver_label(&self, view: &crate::runtime::AudioSettingsView) -> String {
        view.drivers
            .iter()
            .find(|driver| driver.id == self.audio_draft.driver)
            .map_or_else(
                || {
                    if self.audio_draft.driver.is_empty() {
                        "Select a driver".into()
                    } else {
                        self.audio_draft.driver.clone()
                    }
                },
                |driver| driver.label.clone(),
            )
    }

    fn device_label(&self, view: &crate::runtime::AudioSettingsView) -> String {
        self.devices_for(view)
            .iter()
            .find(|device| device.id == self.audio_draft.device)
            .map_or_else(
                || {
                    if self.audio_draft.device.is_empty() {
                        "Select a device".into()
                    } else {
                        self.audio_draft.device.clone()
                    }
                },
                |device| device.label.clone(),
            )
    }

    fn sample_rate_label(&self, _view: &crate::runtime::AudioSettingsView) -> String {
        if self.audio_draft.sample_rate == "auto" || self.audio_draft.sample_rate.is_empty() {
            "Automatic".into()
        } else {
            format!("{} Hz", self.audio_draft.sample_rate)
        }
    }

    fn devices_for<'a>(
        &self,
        view: &'a crate::runtime::AudioSettingsView,
    ) -> &'a [crate::runtime::AudioDeviceView] {
        view.drivers
            .iter()
            .find(|driver| driver.id == self.audio_draft.driver)
            .map_or(&[], |driver| driver.devices.as_slice())
    }

    fn sample_rates_for(&self, view: &crate::runtime::AudioSettingsView) -> Vec<u32> {
        self.devices_for(view)
            .iter()
            .find(|device| device.id == self.audio_draft.device)
            .map_or_else(Vec::new, |device| device.sample_rates.clone())
    }
}
