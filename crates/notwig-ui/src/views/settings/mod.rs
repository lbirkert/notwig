use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Align2, Color32, FontId, RichText, Vec2};
use notwig_ext_api::{ExtensionIndexEntry, ExtensionInfo, ExtensionStatus, target};

use crate::form;
use crate::intent::UiIntent;
use crate::runtime::{AudioChannelConfig, AudioChannelDirection, AudioChannelFormat, RuntimeView};
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

    fn extensions_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        if !runtime.extension_index_loaded
            && !runtime.extension_index_loading
            && !self.extension_index_requested
            && !runtime.extension_indexes.is_empty()
        {
            self.extension_index_requested = true;
            self.emit(UiIntent::RefreshExtensionIndex);
        }

        ui.horizontal(|ui| {
            ui.label(theme::page_heading(ui.ctx(), "Extensions"));
            ui.add_space(8.0);
            if ui.button("Add .ZIP").clicked() {
                self.emit(UiIntent::ImportExtensionZip);
            }
            if ui.button("Manage Indices").clicked() {
                self.show_extension_index_manager = true;
            }
        });
        ui.add_space(10.0);

        let cards = extension_cards(runtime);
        let installed_count = cards.iter().filter(|card| card.installed.is_some()).count();
        let available_count = cards.len().saturating_sub(installed_count);
        ui.horizontal(|ui| {
            if extension_filter_tab(
                ui,
                "All",
                cards.len(),
                self.extension_filter == ExtensionFilter::All,
            ) {
                self.extension_filter = ExtensionFilter::All;
            }
            ui.add_space(18.0);
            if extension_filter_tab(
                ui,
                "Available",
                available_count,
                self.extension_filter == ExtensionFilter::Available,
            ) {
                self.extension_filter = ExtensionFilter::Available;
            }
            ui.add_space(18.0);
            if extension_filter_tab(
                ui,
                "Installed",
                installed_count,
                self.extension_filter == ExtensionFilter::Installed,
            ) {
                self.extension_filter = ExtensionFilter::Installed;
            }
            if runtime.extension_index_loading {
                ui.add_space(12.0);
                ui.label(RichText::new("Loading…").small().color(theme::TEXT_MUTED));
            }
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(8.0);

        if !runtime.extension_index_errors.is_empty() {
            ui.label(
                RichText::new(
                    "Some index sources failed to load. Open Manage Indices for details.",
                )
                .small()
                .color(theme::PLAYHEAD),
            );
            ui.add_space(4.0);
        }

        if !runtime.extension_errors.is_empty() {
            ui.label(
                RichText::new("Some extensions failed to load:")
                    .small()
                    .color(theme::PLAYHEAD),
            );
            for error in &runtime.extension_errors {
                ui.label(RichText::new(error).small().color(theme::PLAYHEAD));
            }
            ui.add_space(6.0);
        }

        let filtered: Vec<ExtensionCard> = cards
            .into_iter()
            .filter(|card| match self.extension_filter {
                ExtensionFilter::All => true,
                ExtensionFilter::Available => card.installed.is_none(),
                ExtensionFilter::Installed => card.installed.is_some(),
            })
            .collect();
        if filtered.is_empty() {
            let message = match self.extension_filter {
                ExtensionFilter::All => {
                    "No extensions found. Add an index source or import a .ZIP."
                }
                ExtensionFilter::Available => "No available extensions.",
                ExtensionFilter::Installed => "No installed extensions.",
            };
            ui.label(RichText::new(message).small().color(theme::TEXT_MUTED));
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("extensions-list")
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for card in &filtered {
                    self.extension_card(ui, card);
                    ui.add_space(8.0);
                }
            });
    }

    fn extension_card(&mut self, ui: &mut egui::Ui, card: &ExtensionCard) {
        let installed = card.installed.clone();
        let disabled = installed.as_ref().is_some_and(|info| !info.enabled);
        let target_tags = extension_target_labels(installed.as_ref(), card.index_entry.as_ref());
        let target_unavailable =
            !extension_target_supported(installed.as_ref(), card.index_entry.as_ref());
        let update_available =
            extension_update_available(installed.as_ref(), card.index_entry.as_ref());
        let inner = ui.scope(|ui| {
            if disabled || target_unavailable {
                ui.set_opacity(0.55);
            }
            egui::Frame::new()
                .fill(if installed.is_some() {
                    theme::PANEL_LIGHT
                } else {
                    theme::PANEL
                })
                .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
                .corner_radius(egui::CornerRadius::same(9))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.extension_thumbnail(ui, card, Vec2::splat(72.0));
                        ui.add_space(10.0);
                        let action_width = 126.0;
                        ui.allocate_ui_with_layout(
                            Vec2::new((ui.available_width() - action_width).max(140.0), 72.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&card.name)
                                            .font(theme::custom_font(
                                                ui.ctx(),
                                                "Geist SemiBold",
                                                17.0,
                                            ))
                                            .color(theme::TEXT_PRIMARY),
                                    );
                                    ui.label(
                                        RichText::new(format!("by {}", card.vendor))
                                            .size(14.0)
                                            .color(theme::TEXT_MUTED),
                                    );
                                });
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(
                                        card.description
                                            .as_deref()
                                            .unwrap_or("No description provided."),
                                    )
                                    .small()
                                    .color(theme::TEXT_MUTED),
                                );
                                ui.add_space(3.0);
                                let version = installed
                                    .as_ref()
                                    .map_or(card.version.as_str(), |info| info.version.as_str());
                                let device_count = installed.as_ref().map_or_else(
                                    || {
                                        card.index_entry
                                            .as_ref()
                                            .map_or(0, |entry| entry.devices.len())
                                    },
                                    |info| info.device_count,
                                );
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "v{version} · {device_count} device{}",
                                            if device_count == 1 { "" } else { "s" }
                                        ))
                                        .small()
                                        .color(theme::TEXT_MUTED),
                                    );
                                    for tag in &target_tags {
                                        extension_target_tag_widget(ui, tag);
                                    }
                                });
                                if target_unavailable {
                                    ui.label(
                                        RichText::new("Not available for this platform")
                                            .small()
                                            .color(theme::PLAYHEAD),
                                    );
                                } else if let Some(info) = installed.as_ref()
                                    && info.status == ExtensionStatus::Failed
                                {
                                    ui.label(
                                        RichText::new(
                                            info.error
                                                .as_deref()
                                                .unwrap_or("Failed to load this extension."),
                                        )
                                        .small()
                                        .color(theme::PLAYHEAD),
                                    );
                                }
                            },
                        );
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            if installed.is_some() {
                                let button = egui::Button::new(
                                    RichText::new("Uninstall").color(theme::ACCENT),
                                )
                                .stroke(egui::Stroke::new(1.0_f32, theme::ACCENT));
                                if ui.add_sized([96.0, 30.0], button).clicked() {
                                    self.pending_extension_replace = None;
                                    self.emit(UiIntent::UninstallExtension {
                                        id: card.id.clone(),
                                    });
                                }
                                if update_available && !target_unavailable {
                                    ui.add_space(4.0);
                                    let update = egui::Button::new(
                                        RichText::new("Update").color(theme::ACCENT),
                                    )
                                    .stroke(egui::Stroke::new(1.0_f32, theme::ACCENT));
                                    if ui.add_sized([96.0, 30.0], update).clicked() {
                                        self.emit(UiIntent::InstallExtensionFromIndex {
                                            id: card.id.clone(),
                                        });
                                    }
                                }
                            } else if target_unavailable {
                                ui.label(
                                    RichText::new("Unavailable").small().color(theme::PLAYHEAD),
                                );
                            } else if ui
                                .add_sized([96.0, 30.0], egui::Button::new("Install"))
                                .clicked()
                            {
                                self.emit(UiIntent::InstallExtensionFromIndex {
                                    id: card.id.clone(),
                                });
                            }
                        });
                    });
                    if installed.is_some()
                        && self.pending_extension_replace.as_deref() == Some(card.id.as_str())
                    {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Replace installed extension?")
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                            if ui.button("Replace").clicked() {
                                self.pending_extension_replace = None;
                                self.emit(UiIntent::InstallExtensionFromIndex {
                                    id: card.id.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked() {
                                self.pending_extension_replace = None;
                            }
                        });
                    }
                });
        });

        inner.response.context_menu(|ui| {
            if let Some(info) = installed.as_ref() {
                let enabled = info.enabled;
                if ui
                    .button(if enabled { "Disable" } else { "Enable" })
                    .clicked()
                {
                    self.emit(UiIntent::SetExtensionEnabled {
                        id: info.id.clone(),
                        enabled: !enabled,
                    });
                    ui.close();
                }
                if card.index_entry.is_some() && ui.button("Reinstall").clicked() {
                    self.pending_extension_replace = Some(info.id.clone());
                    ui.close();
                }
                if ui.button("Uninstall").clicked() {
                    self.emit(UiIntent::UninstallExtension {
                        id: info.id.clone(),
                    });
                    ui.close();
                }
            } else if !target_unavailable && ui.button("Install").clicked() {
                self.emit(UiIntent::InstallExtensionFromIndex {
                    id: card.id.clone(),
                });
                ui.close();
            }
        });
    }

    fn extension_thumbnail(&mut self, ui: &mut egui::Ui, card: &ExtensionCard, size: Vec2) {
        if let Some(bytes) = card.icon.as_deref() {
            if let Some(texture) = self.extension_icon_texture(ui.ctx(), &card.id, bytes) {
                ui.add(
                    egui::Image::new(&texture)
                        .fit_to_exact_size(size)
                        .corner_radius(egui::CornerRadius::same(8)),
                );
                return;
            }
        }
        if let Some(path) = card.image_path.as_deref() {
            if let Some(texture) = self.extension_texture(ui.ctx(), path) {
                ui.add(
                    egui::Image::new(&texture)
                        .fit_to_exact_size(size)
                        .corner_radius(egui::CornerRadius::same(8)),
                );
                return;
            }
        }
        extension_placeholder(ui, size);
    }

    fn extension_icon_texture(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        bytes: &[u8],
    ) -> Option<egui::TextureHandle> {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let fingerprint = hasher.finish();
        if let Some((cached, texture)) = self.extension_icon_images.get(id) {
            if *cached == fingerprint {
                return texture.clone();
            }
        }
        let texture = image::load_from_memory(bytes).ok().map(|image| {
            let image = image.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture(
                format!("extension-icon:{id}"),
                color,
                egui::TextureOptions::LINEAR,
            )
        });
        self.extension_icon_images
            .insert(id.to_owned(), (fingerprint, texture.clone()));
        texture
    }

    fn extension_texture(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
    ) -> Option<egui::TextureHandle> {
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok();
        if let Some((cached_modified, texture)) = self.extension_images.get(path) {
            if *cached_modified == modified {
                return texture.clone();
            }
        }
        let texture = image::open(path).ok().map(|image| {
            let image = image.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture(
                format!("extension-image:{}", path.display()),
                color,
                egui::TextureOptions::LINEAR,
            )
        });
        self.extension_images
            .insert(path.to_path_buf(), (modified, texture.clone()));
        texture
    }

    fn extension_index_manager_window(&mut self, ctx: &egui::Context, runtime: &RuntimeView) {
        let mut open = self.show_extension_index_manager;
        egui::Window::new("Manage Indices")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Available extensions are fetched from these JSON index files.")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(8.0);
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
                            .desired_width(360.0),
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
            });
        self.show_extension_index_manager = open;
    }

    fn audio_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        let view = &runtime.audio_settings;
        ui.label(theme::page_heading(ui.ctx(), "Audio"));
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
            self.audio_draft.channels.clone_from(&current.channels);
            self.audio_draft
                .master_output
                .clone_from(&current.master_output);
            self.audio_draft.low_latency_block_size = current.low_latency_block_size;
            self.audio_draft.low_latency_monitoring = current.low_latency_monitoring;
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
            .map(|device| {
                (
                    device.id.clone(),
                    format!(
                        "{} · {}-in, {}-out",
                        device.label, device.input_channels, device.output_channels
                    ),
                )
            })
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

        if self.audio_draft.block_size == 0 {
            self.audio_draft.block_size = 512;
        }
        if self.audio_draft.low_latency_block_size == 0 {
            self.audio_draft.low_latency_block_size = 16;
        }
        self.audio_draft.low_latency_block_size = self
            .audio_draft
            .low_latency_block_size
            .clamp(16, self.audio_draft.block_size);
        let rate = self
            .audio_draft
            .sample_rate
            .parse::<u32>()
            .ok()
            .or_else(|| current.sample_rate.parse::<u32>().ok())
            .unwrap_or(48_000);
        let latency_ms = |samples: u32| f64::from(samples) / f64::from(rate) * 1000.0;
        let full_width = ui.max_rect().width().max(ui.available_width());
        ui.checkbox(
            &mut self.audio_draft.low_latency_monitoring,
            "Enable low latency monitoring",
        );
        let max_block_size = if self.audio_draft.low_latency_monitoring {
            crate::MAX_AUDIO_BLOCK_SIZE
        } else {
            crate::MAX_AUDIO_BLOCK_SIZE_WITHOUT_LOW_LATENCY
        };
        self.audio_draft.block_size = self.audio_draft.block_size.clamp(16, max_block_size);
        self.audio_draft.low_latency_block_size = self
            .audio_draft
            .low_latency_block_size
            .clamp(16, self.audio_draft.block_size);

        ui.label(RichText::new("Block size").size(13.0));
        styled_buffer_slider(
            ui,
            &mut self.audio_draft.block_size,
            max_block_size,
            full_width,
        );
        ui.label(
            RichText::new(format!(
                "{} samples / {:.2} ms",
                self.audio_draft.block_size,
                latency_ms(self.audio_draft.block_size)
            ))
            .monospace()
            .color(theme::ACCENT),
        );
        ui.add_space(8.0);
        ui.add_enabled_ui(self.audio_draft.low_latency_monitoring, |ui| {
            ui.label(RichText::new("Low latency buffer").size(13.0));
            styled_buffer_slider(
                ui,
                &mut self.audio_draft.low_latency_block_size,
                self.audio_draft.block_size,
                full_width,
            );
            ui.label(
                RichText::new(format!(
                    "{} samples / {:.2} ms",
                    self.audio_draft.low_latency_block_size,
                    latency_ms(self.audio_draft.low_latency_block_size)
                ))
                .monospace()
                .color(theme::ACCENT),
            );
        });
        ui.add_space(14.0);
        self.audio_io_settings(ui, runtime);
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
                    low_latency_block_size: self.audio_draft.low_latency_block_size,
                    low_latency_monitoring: self.audio_draft.low_latency_monitoring,
                    channels: self.audio_draft.channels.clone(),
                    master_output: self.audio_draft.master_output.clone(),
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

    fn audio_io_settings(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
        form::section_heading(ui, "Inputs & Outputs");
        let selected = selected_device(runtime, &self.audio_draft);
        let input_channels = selected.map_or(0, |device| device.input_channels);
        let output_channels = selected.map_or(0, |device| device.output_channels);
        let device_label = selected.map_or_else(
            || "the selected audio device".to_string(),
            |device| device.label.clone(),
        );
        ui.label(
            RichText::new(format!(
                "Name channels on {device_label}. Named inputs appear in the track input picker for recording."
            ))
            .small()
            .color(theme::TEXT_MUTED),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (direction, format, available) in [
                (
                    AudioChannelDirection::Input,
                    AudioChannelFormat::Mono,
                    input_channels,
                ),
                (
                    AudioChannelDirection::Input,
                    AudioChannelFormat::Stereo,
                    input_channels,
                ),
                (
                    AudioChannelDirection::Output,
                    AudioChannelFormat::Mono,
                    output_channels,
                ),
                (
                    AudioChannelDirection::Output,
                    AudioChannelFormat::Stereo,
                    output_channels,
                ),
            ] {
                let label = format!(
                    "Add {} {}",
                    format.label().to_lowercase(),
                    direction.label().to_lowercase()
                );
                let enabled = available >= format.channel_count();
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    add_audio_channel(&mut self.audio_draft, direction, format, available);
                }
            }
        });
        ui.add_space(8.0);

        let mut channels = self.audio_draft.channels.clone();
        for channel in &mut channels {
            channel.driver.clone_from(&self.audio_draft.driver);
            channel.device.clone_from(&self.audio_draft.device);
            let available = if channel.direction == AudioChannelDirection::Input {
                input_channels
            } else {
                output_channels
            };
            normalize_channel_channels(channel, available);
        }
        if channels.is_empty() {
            ui.label(
                RichText::new("No named channels yet.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else {
            let mut remove = None;
            for channel in &mut channels {
                let available = if channel.direction == AudioChannelDirection::Input {
                    input_channels
                } else {
                    output_channels
                };
                normalize_channel_channels(channel, available);
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut channel.name)
                                    .hint_text("Channel name")
                                    .desired_width(150.0),
                            );
                            ui.label(
                                RichText::new(channel.kind_label())
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                            if channel.channel_count() >= 2 {
                                ui.label(RichText::new("L").small().color(theme::TEXT_MUTED));
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-left", &channel.id),
                                    &mut channel.left_channel,
                                    available,
                                    None,
                                );
                                ui.label(RichText::new("R").small().color(theme::TEXT_MUTED));
                                let mut right = channel
                                    .right_channel
                                    .unwrap_or_else(|| channel.left_channel.saturating_add(1));
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-right", &channel.id),
                                    &mut right,
                                    available,
                                    Some(channel.left_channel),
                                );
                                channel.right_channel = Some(right);
                            } else {
                                channel_channel_combo(
                                    ui,
                                    ("audio-channel-left", &channel.id),
                                    &mut channel.left_channel,
                                    available,
                                    None,
                                );
                                channel.right_channel = None;
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(channel.id.clone());
                            }
                        });
                    });
                ui.add_space(4.0);
            }
            if let Some(id) = remove {
                channels.retain(|channel| channel.id != id);
                if self.audio_draft.master_output.as_deref() == Some(id.as_str()) {
                    self.audio_draft.master_output = None;
                }
            }
        }
        self.audio_draft.channels = channels;

        let output_channels: Vec<(String, String)> = self
            .audio_draft
            .channels
            .iter()
            .filter(|channel| channel.direction == AudioChannelDirection::Output)
            .map(|channel| (channel.id.clone(), channel.name.clone()))
            .collect();
        if !output_channels.is_empty() {
            ui.add_space(4.0);
            let mut master = self.audio_draft.master_output.clone();
            let selected = master
                .as_ref()
                .and_then(|id| {
                    output_channels
                        .iter()
                        .find(|(channel_id, _)| channel_id == id)
                        .map(|(_, name)| name.clone())
                })
                .unwrap_or_else(|| "Default output".to_string());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Master output").size(13.0));
                egui::ComboBox::from_id_salt("audio-master-output")
                    .selected_text(selected)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(master.is_none(), "Default output")
                            .clicked()
                        {
                            master = None;
                        }
                        for (id, name) in &output_channels {
                            if ui
                                .selectable_label(master.as_deref() == Some(id.as_str()), name)
                                .clicked()
                            {
                                master = Some(id.clone());
                            }
                        }
                    });
            });
            self.audio_draft.master_output = master;
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

fn styled_buffer_slider(ui: &mut egui::Ui, value: &mut u32, max: u32, full_width: f32) {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = full_width;
        ui.visuals_mut().selection.bg_fill = theme::ACCENT;
        ui.visuals_mut().widgets.inactive.bg_fill = theme::GRID_STRONG;
        ui.add_sized(
            [full_width, ui.spacing().interact_size.y.max(20.0)],
            egui::Slider::new(value, 16..=max)
                .step_by(16.0)
                .show_value(false)
                .trailing_fill(true),
        );
    });
}

fn selected_device<'a>(
    runtime: &'a RuntimeView,
    draft: &crate::runtime::AudioSettingsValue,
) -> Option<&'a crate::runtime::AudioDeviceView> {
    runtime
        .audio_settings
        .drivers
        .iter()
        .find(|driver| driver.id == draft.driver)
        .and_then(|driver| {
            driver
                .devices
                .iter()
                .find(|device| device.id == draft.device)
        })
}

fn normalize_channel_channels(channel: &mut AudioChannelConfig, available: u16) {
    if available == 0 {
        return;
    }
    channel.left_channel = channel.left_channel.min(available - 1);
    if channel.channel_count() >= 2 {
        if available < 2 {
            channel.format = AudioChannelFormat::Mono;
            channel.right_channel = None;
            return;
        }
        let mut right = channel
            .right_channel
            .unwrap_or_else(|| channel.left_channel.saturating_add(1))
            .min(available - 1);
        if right == channel.left_channel {
            right = (channel.left_channel + 1) % available;
        }
        channel.right_channel = Some(right);
    } else {
        channel.right_channel = None;
    }
}

fn channel_channel_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    channel: &mut u16,
    available: u16,
    exclude: Option<u16>,
) {
    let available = available.max(1);
    if *channel >= available {
        *channel = 0;
    }
    if exclude == Some(*channel) {
        *channel = (0..available)
            .find(|candidate| Some(*candidate) != exclude)
            .unwrap_or(*channel);
    }
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("Ch {}", *channel + 1))
        .width(72.0)
        .show_ui(ui, |ui| {
            for candidate in 0..available {
                if Some(candidate) == exclude {
                    continue;
                }
                if ui
                    .selectable_label(candidate == *channel, format!("Ch {}", candidate + 1))
                    .clicked()
                {
                    *channel = candidate;
                }
            }
        });
}

fn add_audio_channel(
    draft: &mut crate::runtime::AudioSettingsValue,
    direction: AudioChannelDirection,
    format: AudioChannelFormat,
    available: u16,
) {
    if available < format.channel_count() {
        return;
    }
    let used = draft
        .channels
        .iter()
        .filter(|channel| channel.direction == direction)
        .map(|channel| {
            let right = channel.right_channel.unwrap_or(channel.left_channel);
            channel.left_channel.max(right).saturating_add(1)
        })
        .max()
        .unwrap_or(0);
    let left_channel = used.min(available.saturating_sub(format.channel_count()));
    let right_channel = (format.channel_count() >= 2).then(|| left_channel + 1);
    let count = draft
        .channels
        .iter()
        .filter(|channel| channel.direction == direction && channel.format == format)
        .count()
        + 1;
    draft.channels.push(AudioChannelConfig {
        id: format!("channel-{}", uuid::Uuid::new_v4()),
        name: format!("{} {} {count}", format.label(), direction.label()),
        direction,
        format,
        driver: draft.driver.clone(),
        device: draft.device.clone(),
        left_channel,
        right_channel,
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ExtensionTargetTag {
    Wasm,
    Macos,
    Windows,
    Linux,
}

impl ExtensionTargetTag {
    const fn label(self) -> &'static str {
        match self {
            Self::Wasm => "WASM",
            Self::Macos => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
        }
    }
}

fn extension_target_tag(raw: &str) -> Option<ExtensionTargetTag> {
    let canonical = target::canonical(raw)?;
    if canonical == target::WASM {
        Some(ExtensionTargetTag::Wasm)
    } else if canonical.starts_with("macos-") {
        Some(ExtensionTargetTag::Macos)
    } else if canonical.starts_with("windows-") {
        Some(ExtensionTargetTag::Windows)
    } else if canonical.starts_with("linux-") {
        Some(ExtensionTargetTag::Linux)
    } else {
        None
    }
}

pub fn extension_target_labels(
    installed: Option<&ExtensionInfo>,
    index_entry: Option<&ExtensionIndexEntry>,
) -> Vec<&'static str> {
    let mut tags = std::collections::BTreeSet::new();
    if let Some(info) = installed {
        for raw in &info.targets {
            if let Some(tag) = extension_target_tag(raw) {
                tags.insert(tag);
            }
        }
    }
    if let Some(entry) = index_entry {
        for url in &entry.urls {
            for raw in &url.targets {
                if let Some(tag) = extension_target_tag(raw) {
                    tags.insert(tag);
                }
            }
        }
    }
    tags.into_iter().map(ExtensionTargetTag::label).collect()
}

pub fn extension_target_supported(
    installed: Option<&ExtensionInfo>,
    index_entry: Option<&ExtensionIndexEntry>,
) -> bool {
    if let Some(info) = installed
        && !info.targets.is_empty()
    {
        return info.targets.iter().any(|raw| target::is_supported(raw));
    }
    if let Some(entry) = index_entry {
        let mut saw_target = false;
        for url in &entry.urls {
            for raw in &url.targets {
                saw_target = true;
                if target::is_supported(raw) {
                    return true;
                }
            }
        }
        if saw_target {
            return false;
        }
    }
    true
}

pub fn extension_update_available(
    installed: Option<&ExtensionInfo>,
    index_entry: Option<&ExtensionIndexEntry>,
) -> bool {
    let Some(info) = installed else {
        return false;
    };
    let Some(hash) = info.installed_hash.as_deref() else {
        return false;
    };
    let Some(entry) = index_entry else {
        return false;
    };
    let mut saw_target = false;
    for url in &entry.urls {
        if url.targets.iter().any(|raw| target::is_supported(raw)) {
            saw_target = true;
            if url.hash.eq_ignore_ascii_case(hash) {
                return false;
            }
        }
    }
    saw_target
}

fn extension_target_tag_widget(ui: &mut egui::Ui, label: &str) {
    egui::Frame::new()
        .fill(theme::PANEL_LIGHT)
        .stroke(egui::Stroke::new(1.0_f32, theme::GRID))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(9.5).color(theme::TEXT_MUTED));
        });
}

struct ExtensionCard {
    id: String,
    name: String,
    vendor: String,
    version: String,
    description: Option<String>,
    image_path: Option<PathBuf>,
    icon: Option<Arc<Vec<u8>>>,
    installed: Option<ExtensionInfo>,
    index_entry: Option<ExtensionIndexEntry>,
}

fn extension_cards(runtime: &RuntimeView) -> Vec<ExtensionCard> {
    let mut cards = Vec::new();
    let mut seen = BTreeSet::new();
    for info in runtime.extensions.clone() {
        let index_entry = runtime
            .extension_index
            .iter()
            .find(|entry| entry.id == info.id)
            .cloned();
        seen.insert(info.id.clone());
        let icon = runtime.extension_icons.get(&info.id).cloned();
        cards.push(ExtensionCard {
            id: info.id.clone(),
            name: info.name.clone(),
            vendor: info.vendor.clone(),
            version: info.version.clone(),
            description: info.description.clone().or_else(|| {
                index_entry
                    .as_ref()
                    .and_then(|entry| entry.description.clone())
            }),
            image_path: info.image_path.clone(),
            icon,
            installed: Some(info),
            index_entry,
        });
    }
    for entry in runtime.extension_index.clone() {
        if seen.contains(&entry.id) {
            continue;
        }
        let icon = runtime.extension_icons.get(&entry.id).cloned();
        cards.push(ExtensionCard {
            id: entry.id.clone(),
            name: entry.name.clone(),
            vendor: entry.vendor.clone(),
            version: entry.version.clone(),
            description: entry.description.clone(),
            image_path: None,
            icon,
            installed: None,
            index_entry: Some(entry),
        });
    }
    cards.sort_by_key(|left| left.name.to_lowercase());
    cards
}

fn extension_filter_tab(ui: &mut egui::Ui, label: &str, count: usize, selected: bool) -> bool {
    ui.horizontal(|ui| {
        let label_response = ui.selectable_label(selected, RichText::new(label).size(17.0));
        let (rect, badge_response) =
            ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
        let color = if selected {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        };
        let painter = ui.painter();
        painter.circle_stroke(rect.center(), 11.0, egui::Stroke::new(1.5_f32, color));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            count.to_string(),
            FontId::monospace(11.0),
            color,
        );
        label_response.clicked() || badge_response.clicked()
    })
    .inner
}

fn extension_placeholder(ui: &mut egui::Ui, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(8), theme::PANEL_LIGHT);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0_f32, theme::GRID_STRONG),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "IMG",
        FontId::monospace(12.0),
        theme::TEXT_MUTED,
    );
}

#[doc(hidden)]
pub mod test_api {
    pub use super::{
        extension_target_labels, extension_target_supported, extension_update_available,
    };
}
