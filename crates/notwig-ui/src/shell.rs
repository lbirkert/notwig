use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, Rect, RichText};
use notwig_core::{ProjectDoc, TrackId, TrackKind};
use notwig_dsp::DeviceRegistry;

use crate::arranger::ClipDrag;
use crate::device::EqDrag;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::{NotificationKind, UiState};
use crate::theme;
use crate::widgets::{format_beats, small_toggle};

pub struct NotwigUi {
    pub state: UiState,
    pub intents: Vec<UiIntent>,
    pub registry: DeviceRegistry,
    pub drag: Option<ClipDrag>,
    pub clip_preview: Option<(notwig_core::ClipId, notwig_core::RangeMus, TrackId)>,
    pub show_notifications: bool,
    pub show_help: bool,
    pub track_rows: BTreeMap<TrackId, Rect>,
    pub timeline_width: f32,
    pub timeline_left: f32,
    pub arranger_rect: Rect,
    pub loop_drag_start: Option<notwig_core::TimeMus>,
    pub eq_band: usize,
    pub eq_range: f32,
    pub eq_drag: Option<EqDrag>,
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
            show_notifications: false,
            show_help: false,
            track_rows: BTreeMap::new(),
            timeline_width: 800.0,
            timeline_left: 230.0,
            arranger_rect: Rect::NOTHING,
            loop_drag_start: None,
            eq_band: 1,
            eq_range: 10.0,
            eq_drag: None,
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
        self.track_rows.clear();
        self.sanitize(doc);
        self.handle_dropped_files(ctx, doc);
        self.handle_shortcuts(ctx, doc, runtime);
        self.top_bar(ctx, doc, runtime);
        self.status_bar(ctx, doc, runtime);
        self.device_panel(ctx, doc, runtime);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| self.arranger_view(ui, doc, runtime));
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

    fn handle_shortcuts(&mut self, ctx: &egui::Context, doc: &ProjectDoc, runtime: &RuntimeView) {
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
            if ctx.input(|i| i.key_pressed(egui::Key::Num0)) {
                self.zoom_to_fit(doc);
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
        let _ = runtime;
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
                    ui.separator();
                    let title = if runtime.dirty {
                        format!("{} *", doc.meta.name)
                    } else {
                        doc.meta.name.clone()
                    };
                    ui.label(RichText::new(title).strong());
                    ui.separator();
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
                    let mut loop_value = doc.transport.loop_enabled;
                    if small_toggle(ui, "L", &mut loop_value, theme::ACCENT) {
                        self.emit(UiIntent::ToggleLoop);
                    }
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("TEMPO")
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new(format!("{:.2} BPM", doc.meta.tempo_bpm))
                                .strong()
                                .monospace(),
                        );
                    });
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("SIGNATURE")
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{}/{}",
                                doc.meta.time_sig_num, doc.meta.time_sig_den
                            ))
                            .strong()
                            .monospace(),
                        );
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("POSITION")
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new(format_beats(runtime.playhead, doc.meta.time_sig_num))
                                .monospace()
                                .size(17.0)
                                .color(if runtime.playing {
                                    theme::ACCENT
                                } else {
                                    Color32::WHITE
                                }),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Render").clicked() {
                            self.emit(UiIntent::RenderMixdown);
                        }
                        if ui.button("Help").clicked() {
                            self.show_help = !self.show_help;
                        }
                        if ui.button("Fit").clicked() {
                            self.zoom_to_fit(doc);
                        }
                        if ui.small_button("+").clicked() {
                            self.state.viewport.pixels_per_beat =
                                (self.state.viewport.pixels_per_beat * 1.25).clamp(4.0, 800.0);
                        }
                        if ui.small_button("−").clicked() {
                            self.state.viewport.pixels_per_beat =
                                (self.state.viewport.pixels_per_beat / 1.25).clamp(4.0, 800.0);
                        }
                        ui.label(
                            RichText::new(format!("xruns {}", runtime.xruns))
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new("1/16 SNAP")
                                .small()
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
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
            if ui.button("Import Audio…").clicked() {
                self.emit(UiIntent::ImportAudio);
                ui.close();
            }
            if ui.button("Render Mixdown…").clicked() {
                self.emit(UiIntent::RenderMixdown);
                ui.close();
            }
        });
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
                    ui.separator();
                    ui.label(
                        RichText::new(
                            "Double-click lane: new clip · Drag edges: trim · Shift+drag ruler: loop · Middle-drag: pan/zoom",
                        )
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
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
                for note in &self.state.notifications {
                    let color = match note.kind {
                        NotificationKind::Info => theme::TEXT_MUTED,
                        NotificationKind::Warning => theme::ACCENT,
                        NotificationKind::Error => egui::Color32::from_rgb(240, 120, 110),
                    };
                    ui.label(RichText::new(&note.message).color(color));
                }
                if ui.button("Clear").clicked() {
                    self.emit(UiIntent::ClearNotifications);
                }
            });
        self.show_notifications = open;
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Keyboard & workflow")
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                for (key, action) in [
                    ("Space", "Play / pause"),
                    ("Shift+Space", "Stop"),
                    ("Cmd+S / Cmd+Shift+S", "Save / save as"),
                    ("Cmd+N / Cmd+O", "New / open project"),
                    ("Cmd+D", "Duplicate selected clip"),
                    ("Delete", "Delete selected clip or track"),
                    ("Cmd+0", "Fit arrangement"),
                    ("Alt while dragging", "Bypass 1/16 snapping"),
                    ("Double-click empty lane", "Create 4-beat MIDI clip"),
                    ("Drag clip body / edges", "Move or trim clip"),
                    ("Right-click clip", "Mute, duplicate, delete"),
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

    pub(crate) fn zoom_to_fit(&mut self, doc: &ProjectDoc) {
        let end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(8.0_f64, f64::max)
            + 1.0;
        self.state.viewport.start = notwig_core::TimeMus::zero();
        self.state.viewport.pixels_per_beat =
            ((self.timeline_width - 4.0) / end as f32).clamp(4.0, 800.0);
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        let (files, pointer) =
            ctx.input(|input| (input.raw.dropped_files.clone(), input.pointer.hover_pos()));
        let Some(pointer) = pointer else {
            return;
        };
        if !self.arranger_rect.contains(pointer) {
            return;
        }
        let Some(track_id) = self.track_at(pointer.y) else {
            return;
        };
        let Some(track) = doc.tracks.get(&track_id) else {
            return;
        };
        if track.kind != TrackKind::Audio {
            return;
        }
        let Some(file) = files.into_iter().find_map(|file| file.path) else {
            return;
        };
        let ppb = self.state.viewport.pixels_per_beat.max(4.0);
        let beat =
            self.state.viewport.start.as_f64() + f64::from((pointer.x - self.timeline_left) / ppb);
        let at = crate::widgets::snap_16(
            notwig_core::TimeMus::new((beat.max(0.0) * 1_000_000.0).round() as i64, 1_000_000),
            false,
        );
        self.emit(UiIntent::DropAudio {
            path: file,
            track_id,
            at,
        });
    }

    pub(crate) fn track_at(&self, y: f32) -> Option<TrackId> {
        self.track_rows
            .iter()
            .find(|(_, rect)| y >= rect.top() && y < rect.bottom())
            .map(|(id, _)| *id)
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
