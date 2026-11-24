use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Align2, FontId, RichText, Vec2};
use notwig_ext_api::{ExtensionIndexEntry, ExtensionInfo, ExtensionStatus, target};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::theme;

use super::{ExtensionFilter, NotwigUi};

impl NotwigUi {
    pub(super) fn extensions_settings_page(&mut self, ui: &mut egui::Ui, runtime: &RuntimeView) {
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
            ui.spacing_mut().item_spacing.x = 18.0;
            if extension_filter_tab(
                ui,
                "All",
                cards.len(),
                self.extension_filter == ExtensionFilter::All,
            ) {
                self.extension_filter = ExtensionFilter::All;
            }
            if extension_filter_tab(
                ui,
                "Available",
                available_count,
                self.extension_filter == ExtensionFilter::Available,
            ) {
                self.extension_filter = ExtensionFilter::Available;
            }
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
                                let size = card
                                    .index_entry
                                    .as_ref()
                                    .and_then(extension_size)
                                    .map(|bytes| format!(" · {}", format_extension_size(bytes)))
                                    .unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "v{version} · {device_count} device{}{size}",
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

    pub(super) fn extension_index_manager_window(
        &mut self,
        ctx: &egui::Context,
        runtime: &RuntimeView,
    ) {
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

fn extension_size(entry: &ExtensionIndexEntry) -> Option<u64> {
    let download = entry
        .download_for_host()
        .or_else(|| entry.download_for_wasm())
        .or_else(|| entry.urls.first())?;
    (download.size > 0).then_some(download.size)
}

fn format_extension_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= MB as u64 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes >= KB as u64 {
        format!("{:.0} KB", bytes as f64 / KB)
    } else {
        format!("{bytes} B")
    }
}

fn extension_filter_tab(ui: &mut egui::Ui, label: &str, count: usize, selected: bool) -> bool {
    const TAB_HEIGHT: f32 = 28.0;
    let max_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), TAB_HEIGHT),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(max_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_min_height(TAB_HEIGHT);
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
        },
    )
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
