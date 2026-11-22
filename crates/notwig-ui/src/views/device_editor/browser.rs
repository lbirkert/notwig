use egui::{Align, Align2, Color32, FontId, Key, Sense, Vec2};
use notwig_core::{DeviceChainId, DeviceKind, ProjectDoc, SchemaVersion, TrackId};

use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::shell::NotwigUi;
use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceBrowserState {
    pub track_id: TrackId,
    pub chain_id: DeviceChainId,
    pub index: usize,
    pub search: String,
    pub selected: usize,
    pub focus_search: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Stock,
    Extension,
    Plugin,
}

#[derive(Debug, Clone)]
struct BrowserEntry {
    name: String,
    type_id: String,
    version: SchemaVersion,
    kind: DeviceKind,
    category: EntryKind,
}

impl NotwigUi {
    pub(crate) fn open_device_browser(
        &mut self,
        track_id: TrackId,
        chain_id: DeviceChainId,
        index: usize,
    ) {
        self.device_browser = Some(DeviceBrowserState {
            track_id,
            chain_id,
            index,
            search: String::new(),
            selected: 0,
            focus_search: true,
        });
    }

    pub(crate) fn device_browser_window(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        let Some(mut state) = self.device_browser.take() else {
            return;
        };
        let mut open = true;
        let mut chosen: Option<BrowserEntry> = None;
        egui::Window::new("Add Device")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(340.0)
            .default_height(440.0)
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .hint_text("Search devices")
                        .desired_width(f32::INFINITY),
                );
                if state.focus_search {
                    search.request_focus();
                    state.focus_search = false;
                }
                if search.changed() {
                    state.selected = 0;
                }
                ui.separator();

                let needle = state.search.to_lowercase();
                let stock: Vec<BrowserEntry> = [
                    (
                        "Tool",
                        notwig_core::dsp::stock::TOOL_TYPE_ID,
                        DeviceKind::AudioFx,
                    ),
                    (
                        "EQ",
                        notwig_core::dsp::stock::EQ_TYPE_ID,
                        DeviceKind::AudioFx,
                    ),
                    (
                        "Chain",
                        notwig_core::dsp::stock::CHAIN_TYPE_ID,
                        DeviceKind::Container,
                    ),
                ]
                .into_iter()
                .filter(|(name, type_id, _)| {
                    needle.is_empty()
                        || name.to_lowercase().contains(&needle)
                        || type_id.contains(&needle)
                })
                .map(|(name, type_id, kind)| BrowserEntry {
                    name: name.into(),
                    type_id: type_id.into(),
                    version: SchemaVersion::V0,
                    kind,
                    category: EntryKind::Stock,
                })
                .collect();
                let extension_specs: Vec<BrowserEntry> = self
                    .registry
                    .iter()
                    .filter(|spec| {
                        spec.type_id != notwig_core::dsp::stock::TOOL_TYPE_ID
                            && spec.type_id != notwig_core::dsp::stock::EQ_TYPE_ID
                            && spec.type_id != notwig_core::dsp::stock::CHAIN_TYPE_ID
                            && !is_plugin_type_id(&spec.type_id)
                    })
                    .filter(|spec| {
                        needle.is_empty()
                            || spec.name.to_lowercase().contains(&needle)
                            || spec.vendor.to_lowercase().contains(&needle)
                            || spec.type_id.contains(&needle)
                    })
                    .map(|spec| BrowserEntry {
                        name: spec.name.clone(),
                        type_id: spec.type_id.clone(),
                        version: spec.version,
                        kind: spec.kind,
                        category: EntryKind::Extension,
                    })
                    .collect();
                let plugins: Vec<BrowserEntry> = runtime
                    .plugins
                    .iter()
                    .filter(|plugin| {
                        needle.is_empty()
                            || plugin.name.to_lowercase().contains(&needle)
                            || plugin.vendor.to_lowercase().contains(&needle)
                            || plugin.type_id.contains(&needle)
                    })
                    .map(|plugin| BrowserEntry {
                        name: plugin.name.clone(),
                        type_id: plugin.type_id.clone(),
                        version: SchemaVersion::V0,
                        kind: plugin.kind,
                        category: EntryKind::Plugin,
                    })
                    .collect();
                let entries: Vec<BrowserEntry> = stock
                    .iter()
                    .chain(extension_specs.iter())
                    .chain(plugins.iter())
                    .cloned()
                    .collect();
                if entries.is_empty() {
                    state.selected = 0;
                } else {
                    state.selected = state.selected.min(entries.len() - 1);
                }

                let mut navigated = false;
                if !entries.is_empty() {
                    let (up, down) = ui.input(|input| {
                        (
                            input.key_pressed(Key::ArrowUp),
                            input.key_pressed(Key::ArrowDown),
                        )
                    });
                    if down {
                        state.selected = (state.selected + 1) % entries.len();
                        navigated = true;
                    } else if up {
                        state.selected = (state.selected + entries.len() - 1) % entries.len();
                        navigated = true;
                    }
                }

                let mut index = 0usize;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if !stock.is_empty() {
                            ui.label(theme::heading(ui.ctx(), "STOCK"));
                            for entry in &stock {
                                if device_row(
                                    ui,
                                    &entry.name,
                                    format_tag(&entry.type_id),
                                    index == state.selected,
                                    navigated,
                                ) {
                                    chosen = Some(entry.clone());
                                }
                                index += 1;
                            }
                        }
                        if !extension_specs.is_empty() {
                            if !stock.is_empty() {
                                ui.add_space(6.0);
                            }
                            ui.label(theme::heading(ui.ctx(), "EXTENSIONS"));
                            for entry in &extension_specs {
                                if device_row(
                                    ui,
                                    &entry.name,
                                    format_tag(&entry.type_id),
                                    index == state.selected,
                                    navigated,
                                ) {
                                    chosen = Some(entry.clone());
                                }
                                index += 1;
                            }
                        }
                        if !plugins.is_empty() {
                            if !stock.is_empty() || !extension_specs.is_empty() {
                                ui.add_space(6.0);
                            }
                            ui.label(theme::heading(ui.ctx(), "PLUGINS"));
                            for entry in &plugins {
                                if device_row(
                                    ui,
                                    &entry.name,
                                    format_tag(&entry.type_id),
                                    index == state.selected,
                                    navigated,
                                ) {
                                    chosen = Some(entry.clone());
                                }
                                index += 1;
                            }
                        }
                    });

                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    if let Some(entry) = entries.get(state.selected) {
                        chosen = Some(entry.clone());
                    }
                }
            });
        if let Some(entry) = chosen {
            match entry.category {
                EntryKind::Plugin => self.emit(UiIntent::InsertPlugin {
                    track_id: state.track_id,
                    chain_id: state.chain_id,
                    type_id: entry.type_id,
                    kind: entry.kind,
                    index: Some(state.index),
                }),
                EntryKind::Stock => {
                    self.emit(UiIntent::InsertStockDevice {
                        track_id: state.track_id,
                        chain_id: state.chain_id,
                        type_id: entry.type_id,
                        kind: entry.kind,
                        index: Some(state.index),
                    });
                }
                EntryKind::Extension => {
                    self.emit(UiIntent::InsertDevice {
                        track_id: state.track_id,
                        chain_id: state.chain_id,
                        type_id: entry.type_id,
                        version: entry.version,
                        kind: entry.kind,
                        index: Some(state.index),
                    });
                }
            }
            self.device_browser = None;
        } else if open {
            self.device_browser = Some(state);
        }
        let _ = doc;
    }
}

fn is_plugin_type_id(type_id: &str) -> bool {
    type_id.starts_with("vst3.")
        || type_id.starts_with("vst2.")
        || type_id.starts_with("au.")
        || type_id.starts_with("au3.")
        || type_id.starts_with("clap.")
        || type_id.starts_with("lv2.")
}

pub(super) fn format_tag(type_id: &str) -> Option<&'static str> {
    if type_id.starts_with("vst3.") {
        Some("VST3")
    } else if type_id.starts_with("vst2.") {
        Some("VST")
    } else if type_id.starts_with("au3.") {
        Some("AU3")
    } else if type_id.starts_with("au.") {
        Some("AU")
    } else if type_id.starts_with("clap.") {
        Some("CLAP")
    } else if type_id.starts_with("lv2.") {
        Some("LV2")
    } else {
        None
    }
}

fn device_row(
    ui: &mut egui::Ui,
    name: &str,
    tag: Option<&str>,
    selected: bool,
    navigated: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, egui::CornerRadius::same(4), theme::ACCENT_DIM);
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), theme::GRID_STRONG);
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 15.0, rect.center().y),
        Vec2::splat(18.0),
    );
    crate::icons::paint(
        painter,
        icon_rect,
        crate::icons::Icon::Device,
        if selected {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        },
    );
    let tag_color = if selected {
        theme::ACCENT
    } else {
        theme::TEXT_MUTED
    };
    let tag_galley =
        tag.map(|tag| painter.layout_no_wrap(tag.to_owned(), FontId::monospace(9.0), tag_color));
    let tag_rect = tag_galley.as_ref().map(|galley| {
        let padding = Vec2::new(5.0, 2.0);
        let size = galley.size() + padding * 2.0;
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - size.x - 6.0, rect.center().y - size.y / 2.0),
            size,
        )
    });
    let name_painter = if let Some(tag_rect) = tag_rect {
        painter.with_clip_rect(egui::Rect::from_min_max(
            rect.min,
            egui::pos2(tag_rect.left() - 4.0, rect.max.y),
        ))
    } else {
        painter.clone()
    };
    name_painter.text(
        egui::pos2(icon_rect.right() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        if selected {
            Color32::WHITE
        } else {
            theme::TEXT_MUTED
        },
    );
    if let (Some(galley), Some(tag_rect)) = (tag_galley, tag_rect) {
        let padding = Vec2::new(5.0, 2.0);
        painter.rect_filled(tag_rect, egui::CornerRadius::same(4), theme::GRID_STRONG);
        painter.galley(tag_rect.min + padding, galley, tag_color);
    }
    if selected && navigated {
        response.scroll_to_me(Some(Align::Center));
    }
    response.clicked()
}
