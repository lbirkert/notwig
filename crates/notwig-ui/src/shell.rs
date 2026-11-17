use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use egui::{Color32, Rect, RichText};
use notwig_core::{ProjectDoc, TrackId, TrackKind};
use notwig_dsp::DeviceRegistry;

use crate::arranger::{BarSelectionDrag, ClipDrag, GroupDrag, TimeDrag};
use crate::device::{DeviceBrowserState, EqDrag};
use crate::drop_target::DragPreview;
use crate::icons::{Icon, icon_button, icon_button_sized};
use crate::intent::{DropAudioTarget, UiIntent};
use crate::runtime::RuntimeView;
use crate::state::{NotificationKind, UiState};
use crate::theme;
use crate::widgets::format_beats;

pub struct NotwigUi {
    pub state: UiState,
    pub intents: Vec<UiIntent>,
    pub registry: DeviceRegistry,
    pub drag: Option<ClipDrag>,
    pub time_drag: Option<TimeDrag>,
    pub track_drag: Option<TrackId>,
    pub group_drag: Option<GroupDrag>,
    pub bar_selection_drag: Option<BarSelectionDrag>,
    pub group_preview: Vec<(notwig_core::ClipId, TrackId, notwig_core::RangeMus)>,
    pub clip_origins:
        BTreeMap<notwig_core::ClipId, (notwig_core::ClipId, TrackId, notwig_core::RangeMus)>,
    pub clip_preview: Option<(notwig_core::ClipId, notwig_core::RangeMus, TrackId)>,
    pub show_notifications: bool,
    pub show_help: bool,
    pub track_rows: BTreeMap<TrackId, Rect>,
    pub last_track_rows: BTreeMap<TrackId, Rect>,
    pub timeline_width: f32,
    pub timeline_left: f32,
    pub arranger_rect: Rect,
    pub last_pointer: Option<egui::Pos2>,
    pub loop_drag_start: Option<notwig_core::TimeMus>,
    pub eq_band: usize,
    pub eq_range: f32,
    pub eq_drag: Option<EqDrag>,
    pub drag_preview: Option<DragPreview>,
    pub duration_cache: HashMap<PathBuf, Option<f64>>,
    pub expanded_chains: BTreeSet<notwig_core::DeviceChainId>,
    pub collapsed_devices: BTreeSet<notwig_core::DeviceId>,
    pub device_browser: Option<DeviceBrowserState>,
    pub show_info: bool,
    pub platform_pointer: Option<egui::Pos2>,
    pub analyzer_pre: bool,
    pub analyzer_post: bool,
    pub analyzer_resolution: usize,
    pub analyzer_speed: usize,
    pub analyzer_tilt: f32,
    pub analyzer_range: (f32, f32),
    initialized: bool,
}

impl Default for NotwigUi {
    fn default() -> Self {
        Self {
            state: UiState::default(),
            intents: Vec::new(),
            registry: notwig_dsp::stock_registry(),
            drag: None,
            time_drag: None,
            track_drag: None,
            group_drag: None,
            bar_selection_drag: None,
            group_preview: Vec::new(),
            clip_origins: BTreeMap::new(),
            clip_preview: None,
            show_notifications: false,
            show_help: false,
            track_rows: BTreeMap::new(),
            last_track_rows: BTreeMap::new(),
            timeline_width: 800.0,
            timeline_left: 230.0,
            arranger_rect: Rect::NOTHING,
            last_pointer: None,
            loop_drag_start: None,
            eq_band: 1,
            eq_range: 10.0,
            eq_drag: None,
            drag_preview: None,
            duration_cache: HashMap::new(),
            expanded_chains: BTreeSet::new(),
            collapsed_devices: BTreeSet::new(),
            device_browser: None,
            show_info: true,
            platform_pointer: None,
            analyzer_pre: true,
            analyzer_post: false,
            analyzer_resolution: 1,
            analyzer_speed: 1,
            analyzer_tilt: 4.5,
            analyzer_range: (-100.0, 0.0),
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
        if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
            self.last_pointer = Some(pos);
        }
        if let Some(pos) = self.platform_pointer {
            self.last_pointer = Some(pos);
        }
        self.sanitize(doc);
        self.clip_origins = doc
            .arranger
            .clips
            .values()
            .map(|clip| (clip.id, (clip.id, clip.track_id, clip.range)))
            .collect();
        self.last_track_rows = std::mem::take(&mut self.track_rows);
        self.handle_dropped_files(ctx, doc);
        self.update_drag_preview(ctx, doc);
        self.track_rows.clear();
        self.handle_shortcuts(ctx, doc, runtime);
        self.top_bar(ctx, doc, runtime);
        self.status_bar(ctx, doc, runtime);
        self.info_panel(ctx, doc, runtime);
        self.device_panel(ctx, doc, runtime);
        self.device_browser_window(ctx, doc, runtime);
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

    pub fn set_platform_pointer(&mut self, pointer: Option<egui::Pos2>) {
        self.platform_pointer = pointer;
    }

    pub(crate) fn emit(&mut self, intent: UiIntent) {
        if !self.intents.contains(&intent) {
            self.intents.push(intent);
        }
    }

    pub fn selected_track(&self, doc: &ProjectDoc) -> Option<TrackId> {
        self.state
            .selection
            .primary_track()
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
        let time = self
            .state
            .selection
            .time
            .as_ref()
            .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
        let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
        let copy_time = if clips.is_empty() { time.clone() } else { None };
        let (copy_event, cut_event, paste_event) = ctx.input(|input| {
            (
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy)),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Cut)),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_))),
            )
        });
        if copy_event {
            self.emit(UiIntent::CopySelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if cut_event {
            self.emit(UiIntent::CutSelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if paste_event {
            let at = time
                .as_ref()
                .map_or(runtime.playhead, |(range, _)| range.start);
            self.emit(UiIntent::PasteClips { at });
        }
        if space {
            if shift || doc.transport.playing {
                self.emit(UiIntent::Stop);
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
            if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                if let Some((range, _)) = time.clone() {
                    self.emit(UiIntent::SetLoopRange(range));
                } else {
                    self.emit(UiIntent::ToggleLoop);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.emit(UiIntent::Redo);
                } else {
                    self.emit(UiIntent::Undo);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::D)) {
                if let Some((range, tracks)) = time.clone() {
                    self.emit(UiIntent::DuplicateTimeSelection {
                        range,
                        tracks: tracks.clone(),
                    });
                    if let Ok(next) = notwig_core::RangeMus::new(
                        range.start + range.length(),
                        range.end + range.length(),
                    ) {
                        self.state.selection.select_time(next, &tracks);
                    }
                } else {
                    for clip in &clips {
                        self.emit(UiIntent::DuplicateClip { clip_id: *clip });
                    }
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
            let time = self
                .state
                .selection
                .time
                .as_ref()
                .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
            if let Some((range, tracks)) = time {
                self.emit(UiIntent::DeleteTimeSelection { range, tracks });
                self.state.selection.time = None;
                return;
            }
            let devices: Vec<_> = self.state.selection.devices.iter().copied().collect();
            let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
            if !devices.is_empty() {
                for device in devices {
                    self.emit(UiIntent::RemoveDevice(device));
                }
            } else if !clips.is_empty() {
                for clip in clips {
                    self.emit(UiIntent::DeleteClip(clip));
                }
            } else {
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
                    let play_icon = if runtime.playing {
                        Icon::Pause
                    } else {
                        Icon::Play
                    };
                    if icon_button_sized(
                        ui,
                        play_icon,
                        egui::vec2(28.0, 28.0),
                        runtime.playing,
                        if runtime.playing { "Pause" } else { "Play" },
                    )
                    .clicked()
                    {
                        if runtime.playing {
                            self.emit(UiIntent::Pause);
                        } else {
                            self.emit(UiIntent::Play);
                        }
                    }
                    if icon_button(ui, Icon::Stop, false, "Stop").clicked() {
                        self.emit(UiIntent::Stop);
                    }
                    if icon_button(ui, Icon::Loop, doc.transport.loop_enabled, "Toggle loop")
                        .clicked()
                    {
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
                    ui.separator();
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
                        if icon_button(ui, Icon::Info, self.show_info, "Selection info").clicked() {
                            self.show_info = !self.show_info;
                        }
                        if icon_button(ui, Icon::Render, false, "Render mixdown").clicked() {
                            self.emit(UiIntent::RenderMixdown);
                        }
                        if icon_button(ui, Icon::Help, self.show_help, "Keyboard help").clicked() {
                            self.show_help = !self.show_help;
                        }
                        if icon_button(ui, Icon::Fit, false, "Fit arrangement").clicked() {
                            self.zoom_to_fit(doc);
                        }
                        if icon_button(ui, Icon::Plus, false, "Zoom in").clicked() {
                            self.state.viewport.zoom_by(1.25);
                        }
                        if icon_button(ui, Icon::Minus, false, "Zoom out").clicked() {
                            self.state.viewport.zoom_by(1.0 / 1.25);
                        }
                        ui.label(
                            RichText::new(format!("xruns {}", runtime.xruns))
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        let snap = crate::widgets::snap_step(
                            self.state.viewport.pixels_per_beat,
                            doc.meta.time_sig_num,
                        );
                        ui.label(
                            RichText::new(crate::widgets::snap_label(snap, doc.meta.time_sig_num))
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
            if ui.button("New Project (48 kHz)").clicked() {
                self.emit(UiIntent::NewProject);
                ui.close();
            }
            if ui.button("New Project (44.1 kHz)").clicked() {
                self.emit(UiIntent::NewProjectAtRate(44_100));
                ui.close();
            }
            if ui.button("New Project (96 kHz)").clicked() {
                self.emit(UiIntent::NewProjectAtRate(96_000));
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
                    ("Space", "Play / stop (returns to play start)"),
                    ("Shift+Space", "Stop"),
                    ("Cmd+S / Cmd+Shift+S", "Save / save as"),
                    ("Cmd+N / Cmd+O", "New / open project"),
                    ("Cmd+D", "Duplicate selected clips or time range"),
                    (
                        "Cmd+C / Cmd+X / Cmd+V",
                        "Copy / cut / paste clips or time range",
                    ),
                    ("Cmd+Z / Cmd+Shift+Z", "Undo / redo"),
                    ("Cmd+L", "Loop the selected time range"),
                    ("Delete", "Delete selection, clips, or time range"),
                    ("Cmd+0", "Fit arrangement"),
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

    pub(crate) fn zoom_to_fit(&mut self, doc: &ProjectDoc) {
        let end = doc
            .arranger
            .clips
            .values()
            .map(|clip| clip.range.end.as_f64())
            .fold(8.0_f64, f64::max)
            + 1.0;
        self.state.viewport.start = notwig_core::TimeMus::zero();
        self.state.viewport.pixels_per_beat = ((self.timeline_width - 4.0) / end as f32).clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
    }

    fn update_drag_preview(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        let hovered: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .hovered_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        let Some(path) = hovered.first().cloned() else {
            self.drag_preview = None;
            return;
        };
        let Some(pointer) = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()))
        else {
            self.drag_preview = None;
            return;
        };
        if !self.arranger_rect.contains(pointer) {
            self.drag_preview = None;
            return;
        }
        let Some(target) =
            crate::drop_target::resolve_drop_target(doc, &self.last_track_rows, pointer.y)
        else {
            self.drag_preview = None;
            return;
        };
        let seconds = if let Some(cached) = self.duration_cache.get(&path) {
            *cached
        } else {
            let duration = notwig_project::decode_audio(&path)
                .ok()
                .map(|buffer| buffer.frame_count() as f64 / f64::from(buffer.sample_rate.max(1)));
            self.duration_cache.insert(path.clone(), duration);
            duration
        };
        let Some(seconds) = seconds else {
            self.drag_preview = None;
            return;
        };
        let beats = seconds * doc.meta.tempo_bpm / 60.0;
        let duration = notwig_core::TimeMus::new(
            (beats.max(1.0 / 16.0) * 1_000_000.0).round() as i64,
            1_000_000,
        );
        let grid =
            crate::widgets::snap_step(self.state.viewport.pixels_per_beat, doc.meta.time_sig_num);
        self.drag_preview = Some(DragPreview {
            path,
            target,
            at: crate::widgets::snap_time(
                self.state.viewport.x_to_beat(pointer.x, self.timeline_left),
                grid,
                false,
            ),
            duration,
        });
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context, doc: &ProjectDoc) {
        let files: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if files.is_empty() {
            return;
        }
        let preview = self.drag_preview.clone();
        let pointer = self
            .last_pointer
            .or_else(|| ctx.input(|input| input.pointer.latest_pos()));
        let from_pointer = pointer.and_then(|pointer| {
            if !self.arranger_rect.contains(pointer) {
                return None;
            }
            let target =
                crate::drop_target::resolve_drop_target(doc, &self.last_track_rows, pointer.y)?;
            let grid = crate::widgets::snap_step(
                self.state.viewport.pixels_per_beat,
                doc.meta.time_sig_num,
            );
            let at = crate::widgets::snap_time(
                self.state.viewport.x_to_beat(pointer.x, self.timeline_left),
                grid,
                false,
            );
            Some((target, at))
        });
        let Some((first_target, at)) =
            from_pointer.or_else(|| preview.map(|preview| (preview.target, preview.at)))
        else {
            self.state.notify(
                NotificationKind::Warning,
                "Drop audio files onto the timeline.",
            );
            return;
        };
        let order: Vec<TrackId> = doc.track_order.depth_first();
        for (file_index, file) in files.into_iter().enumerate() {
            let target = match (file_index, first_target) {
                (0, target) => target,
                (_, DropAudioTarget::Track(track_id)) => {
                    let index = order
                        .iter()
                        .position(|id| *id == track_id)
                        .map_or(order.len(), |index| index + 1);
                    DropAudioTarget::NewTrack {
                        index: index + file_index - 1,
                    }
                }
                (_, DropAudioTarget::NewTrack { index }) => DropAudioTarget::NewTrack {
                    index: index + file_index,
                },
            };
            self.emit(UiIntent::DropAudio {
                path: file,
                target,
                at,
            });
        }
    }

    pub(crate) fn track_at(&self, y: f32) -> Option<TrackId> {
        self.track_rows
            .iter()
            .chain(self.last_track_rows.iter())
            .find(|(_, rect)| y >= rect.top() && y < rect.bottom())
            .map(|(id, _)| *id)
    }

    pub(crate) fn row_rect(&self, track: TrackId) -> Option<Rect> {
        self.track_rows
            .get(&track)
            .or_else(|| self.last_track_rows.get(&track))
            .copied()
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
