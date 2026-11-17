use std::collections::BTreeSet;

use egui::{Color32, RichText};
use notwig_core::{DeviceId, ProjectDoc, TrackId, TrackKind};
use notwig_dsp::DeviceRegistry;

use crate::arranger::ClipDrag;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::{NotificationKind, UiState, ViewId};
use crate::theme;
use crate::widgets::{format_beats, small_toggle};

pub struct NotwigUi {
    pub state: UiState,
    pub intents: Vec<UiIntent>,
    pub registry: DeviceRegistry,
    pub drag: Option<ClipDrag>,
    pub clip_preview: Option<(notwig_core::ClipId, notwig_core::RangeMus, TrackId)>,
    pub show_browser: bool,
    pub show_inspector: bool,
    pub show_notifications: bool,
    pub show_help: bool,
    initialized: bool,
}

impl Default for NotwigUi {
    fn default() -> Self {
        Self {
            state: UiState::default(),
            intents: Vec::new(),
            registry: notwig_dsp::stock_registry(),
            drag: None,
            clip_preview: None,
            show_browser: true,
            show_inspector: true,
            show_notifications: false,
            show_help: false,
            initialized: false,
        }
    }
}

impl NotwigUi {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) -> Vec<UiIntent> {
        if !self.initialized {
            theme::apply(ctx);
            self.initialized = true;
        }
        self.intents.clear();
        self.sanitize(doc);
        self.handle_shortcuts(ctx, doc);
        self.top_bar(ctx, doc, runtime);
        if self.show_browser {
            self.browser_panel(ctx, doc, runtime);
        }
        if self.show_inspector {
            self.inspector_panel(ctx, doc, runtime);
        }
        self.status_bar(ctx, doc, runtime);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| match self.state.active_view {
                ViewId::Arranger => self.arranger_view(ui, doc, runtime),
                ViewId::Mixer => self.mixer_view(ui, doc, runtime),
                ViewId::Device => self.device_central(ui, doc),
                ViewId::Browser => self.browser_central(ui, doc, runtime),
                ViewId::Settings => self.settings_central(ui),
            });
        if self.show_notifications {
            self.notifications_window(ctx);
        }
        if self.show_help {
            self.help_window(ctx);
        }
        std::mem::take(&mut self.intents)
    }

    pub(crate) fn emit(&mut self, intent: UiIntent) {
        if !self.intents.contains(&intent) {
            self.intents.push(intent);
        }
    }

    pub fn selected_track(&self, doc: &ProjectDoc) -> Option<TrackId> {
        self.state
            .selection
            .primary_track
            .filter(|id| doc.tracks.contains_key(id))
            .or_else(|| {
                self.state
                    .selection
                    .tracks
                    .iter()
                    .copied()
                    .find(|id| doc.tracks.contains_key(id))
            })
            .or_else(|| {
                doc.tracks
                    .keys()
                    .copied()
                    .find(|id| doc.tracks[id].kind != TrackKind::Master)
            })
    }

    fn sanitize(&mut self, doc: &ProjectDoc) {
        self.state
            .selection
            .tracks
            .retain(|id| doc.tracks.contains_key(id));
        self.state
            .selection
            .clips
            .retain(|id| doc.arranger.clips.contains_key(id));
        self.state
            .selection
            .devices
            .retain(|id| doc.devices.contains_key(id));
        if let Some(primary) = self.state.selection.primary_track {
            if !doc.tracks.contains_key(&primary) {
                self.state.selection.primary_track =
                    self.state.selection.tracks.iter().next().copied();
            }
        }
        self.state.selection.notes.clear();
        if self
            .drag
            .as_ref()
            .is_some_and(|drag| !doc.arranger.clips.contains_key(&drag.clip_id))
        {
            self.drag = None;
            self.clip_preview = None;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (space, shift, command, escape, delete) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Space),
                input.modifiers.shift,
                input.modifiers.command,
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
            )
        });
        if space {
            if shift {
                self.emit(UiIntent::Stop);
            } else if doc.transport.playing {
                self.emit(UiIntent::Pause);
            } else {
                self.emit(UiIntent::Play);
            }
        }
        if command {
            if ctx.input(|i| i.key_pressed(egui::Key::S)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.emit(UiIntent::SaveAs);
                } else {
                    self.emit(UiIntent::Save);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::N)) {
                self.emit(UiIntent::NewProject);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::O)) {
                self.emit(UiIntent::OpenProject);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::D)) {
                if let Some(clip) = self.state.selection.clips.iter().next().copied() {
                    self.emit(UiIntent::DuplicateClip { clip_id: clip });
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::B)) {
                self.emit(UiIntent::ToggleBrowser);
            }
        }
        if escape {
            self.state.selection.clear();
        }
        if delete {
            let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
            if clips.is_empty() {
                for track in self
                    .state
                    .selection
                    .tracks
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                {
                    if doc
                        .tracks
                        .get(&track)
                        .is_some_and(|t| t.kind != TrackKind::Master)
                    {
                        self.emit(UiIntent::DeleteTrack(track));
                    }
                }
            } else {
                for clip in clips {
                    self.emit(UiIntent::DeleteClip(clip));
                }
            }
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context, doc: &ProjectDoc, runtime: &RuntimeView) {
        egui::TopBottomPanel::top("notwig_top")
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    self.file_menu(ui);
                    ui.separator();
                    let title = if runtime.dirty {
                        format!("{} *", doc.meta.name)
                    } else {
                        doc.meta.name.clone()
                    };
                    ui.label(RichText::new(title).strong());
                    if let Some(path) = &runtime.bundle_path {
                        ui.label(
                            RichText::new(path.display().to_string())
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .selectable_label(
                                self.state.active_view == ViewId::Settings,
                                "Settings",
                            )
                            .clicked()
                        {
                            self.set_view(ViewId::Settings);
                        }
                        if ui
                            .selectable_label(self.state.active_view == ViewId::Device, "Device")
                            .clicked()
                        {
                            self.set_view(ViewId::Device);
                        }
                        if ui
                            .selectable_label(self.state.active_view == ViewId::Mixer, "Mixer")
                            .clicked()
                        {
                            self.set_view(ViewId::Mixer);
                        }
                        if ui
                            .selectable_label(
                                self.state.active_view == ViewId::Arranger,
                                "Arranger",
                            )
                            .clicked()
                        {
                            self.set_view(ViewId::Arranger);
                        }
                        ui.toggle_value(&mut self.show_inspector, "Inspector");
                        ui.toggle_value(&mut self.show_browser, "Browser");
                    });
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let play_label = if runtime.playing { "Pause" } else { "Play" };
                    let play = egui::Button::new(play_label)
                        .fill(theme::ACCENT)
                        .corner_radius(egui::CornerRadius::same(4));
                    if ui.add(play).clicked() {
                        if runtime.playing {
                            self.emit(UiIntent::Pause);
                        } else {
                            self.emit(UiIntent::Play);
                        }
                    }
                    if ui.button("Stop").clicked() {
                        self.emit(UiIntent::Stop);
                    }
                    let loop_on = doc.transport.loop_enabled;
                    let mut loop_value = loop_on;
                    if small_toggle(ui, "L", &mut loop_value, theme::ACCENT) {
                        self.emit(UiIntent::ToggleLoop);
                    }
                    ui.separator();
                    let beats = format_beats(runtime.playhead, doc.meta.time_sig_num);
                    ui.label(RichText::new(beats).monospace().color(if runtime.playing {
                        theme::ACCENT
                    } else {
                        Color32::WHITE
                    }));
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{:.2} BPM", doc.meta.tempo_bpm))
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{}/{}",
                            doc.meta.time_sig_num, doc.meta.time_sig_den
                        ))
                        .color(theme::TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Render").clicked() {
                            self.emit(UiIntent::RenderMixdown);
                        }
                        if ui.button("Help").clicked() {
                            self.show_help = !self.show_help;
                        }
                        ui.label(
                            RichText::new(format!("xruns {}", runtime.xruns))
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        if let Some(cpu) = runtime.cpu_load {
                            ui.label(
                                RichText::new(format!("cpu {:.1}%", cpu * 100.0))
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            );
                        }
                    });
                });
            });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("New Project").clicked() {
                self.emit(UiIntent::NewProject);
                ui.close();
            }
            if ui.button("Open Project…").clicked() {
                self.emit(UiIntent::OpenProject);
                ui.close();
            }
            if ui.button("Save").clicked() {
                self.emit(UiIntent::Save);
                ui.close();
            }
            if ui.button("Save As…").clicked() {
                self.emit(UiIntent::SaveAs);
                ui.close();
            }
            ui.separator();
            if ui.button("Render Mixdown…").clicked() {
                self.emit(UiIntent::RenderMixdown);
                ui.close();
            }
        });
        ui.menu_button("Track", |ui| {
            if ui.button("Add Audio Track").clicked() {
                self.emit(UiIntent::CreateTrack(TrackKind::Audio));
                ui.close();
            }
            if ui.button("Add Instrument Track").clicked() {
                self.emit(UiIntent::CreateTrack(TrackKind::Instrument));
                ui.close();
            }
            if ui.button("Add Group").clicked() {
                self.emit(UiIntent::CreateTrack(TrackKind::Group));
                ui.close();
            }
            if ui.button("Add FX Bus").clicked() {
                self.emit(UiIntent::CreateTrack(TrackKind::BusFx));
                ui.close();
            }
        });
    }

    fn set_view(&mut self, view: ViewId) {
        self.state.active_view = view;
        self.emit(UiIntent::SetActiveView(view));
    }

    fn status_bar(&mut self, ctx: &egui::Context, doc: &ProjectDoc, runtime: &RuntimeView) {
        egui::TopBottomPanel::bottom("notwig_status")
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} Hz", doc.meta.sample_rate.value()))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.separator();
                    let state = if runtime.playing {
                        "Playing"
                    } else {
                        "Stopped"
                    };
                    ui.label(RichText::new(state).small().color(theme::TEXT_MUTED));
                    if let Some(status) = &runtime.status {
                        ui.separator();
                        ui.label(RichText::new(status).small().color(theme::ACCENT));
                    }
                    if runtime.saving {
                        ui.separator();
                        ui.spinner();
                        ui.label(RichText::new("Saving…").small());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.state.notifications.is_empty()
                            && ui
                                .button(format!("Alerts ({})", self.state.notifications.len()))
                                .clicked()
                        {
                            self.show_notifications = !self.show_notifications;
                        }
                        let selected = self.state.selection.clips.len()
                            + self.state.selection.tracks.len()
                            + self.state.selection.devices.len();
                        ui.label(
                            RichText::new(format!("selected {selected}"))
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                    });
                });
            });
    }

    fn notifications_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_notifications;
        egui::Window::new("Notifications")
            .open(&mut open)
            .default_width(360.0)
            .show(ctx, |ui| {
                if self.state.notifications.is_empty() {
                    ui.label("No notifications.");
                }
                let mut clear = false;
                for note in &self.state.notifications {
                    let color = match note.kind {
                        NotificationKind::Info => theme::TEXT_MUTED,
                        NotificationKind::Warning => theme::ACCENT,
                        NotificationKind::Error => egui::Color32::from_rgb(240, 120, 110),
                    };
                    ui.label(RichText::new(&note.message).color(color));
                }
                if ui.button("Clear").clicked() {
                    clear = true;
                }
                if clear {
                    self.emit(UiIntent::ClearNotifications);
                }
            });
        self.show_notifications = open;
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Keyboard & workflow")
            .open(&mut open)
            .default_width(380.0)
            .show(ctx, |ui| {
                for (key, action) in [
                    ("Space", "Play / pause"),
                    ("Shift+Space", "Stop"),
                    ("Cmd+S / Cmd+Shift+S", "Save / save as"),
                    ("Cmd+N / Cmd+O", "New / open project"),
                    ("Cmd+D", "Duplicate selected clip"),
                    ("Delete", "Delete selected clip or track"),
                    ("Cmd+B", "Toggle browser"),
                    ("Alt while dragging", "Bypass 1/16 snapping"),
                    ("Double-click empty lane", "Create 4-beat MIDI clip"),
                    ("Drag clip edges", "Trim clip"),
                    ("Wheel over ruler", "Zoom arrangement"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).monospace().color(theme::ACCENT));
                        ui.label(action);
                    });
                }
            });
        self.show_help = open;
    }

    #[allow(clippy::unused_self)]
    fn settings_central(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.heading("Settings");
        ui.label(
            RichText::new("Audio device configuration is managed by the application shell.")
                .color(theme::TEXT_MUTED),
        );
        ui.add_space(8.0);
        ui.label("Notwig v0 keeps tempo and time signature fixed for the project lifetime.");
        ui.label("Recording, live input, metronome, and clip launcher are deferred.");
        ui.label("Plugin scan paths and device selection live in the app configuration.");
    }
}

pub(crate) fn track_depth(doc: &ProjectDoc, id: TrackId) -> usize {
    let mut depth = 0usize;
    let mut seen = BTreeSet::new();
    let mut current = doc.tracks.get(&id).and_then(|track| track.parent);
    while let Some(parent) = current {
        if !seen.insert(parent) {
            break;
        }
        depth += 1;
        current = doc.tracks.get(&parent).and_then(|track| track.parent);
    }
    depth
}

pub(crate) fn clip_color(kind: notwig_core::ClipKind) -> Color32 {
    match kind {
        notwig_core::ClipKind::Audio => theme::CLIP_AUDIO,
        notwig_core::ClipKind::Midi => theme::CLIP_MIDI,
    }
}

pub(crate) fn selected_device(doc: &ProjectDoc, ui: &NotwigUi) -> Option<DeviceId> {
    ui.state
        .selection
        .devices
        .iter()
        .copied()
        .find(|id| doc.devices.contains_key(id))
}
