use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use egui::{Color32, Rect, RichText};
use notwig_core::{ProjectDoc, TrackId, TrackKind};
use notwig_dsp::DeviceRegistry;

use crate::arranger::{BarSelectionDrag, ClipDrag, GroupDrag, TimeDrag, TrackDrop};
use crate::clip_editor::ClipEditorState;
use crate::device::{DeviceBrowserState, EqDrag};
use crate::drop_target::DragPreview;
use crate::edit::{DuplicateSpan, EditTarget, duplicate_bar_range, duplicate_plan, paste_target};
use crate::icons::{Icon, icon_button_sized};
use crate::intent::{ClipDuplicate, DropAudioTarget, UiIntent};
use crate::runtime::RuntimeView;
use crate::state::{EditFocus, NotificationKind, UiState};
use crate::theme;

pub struct NotwigUi {
    pub state: UiState,
    pub intents: Vec<UiIntent>,
    pub registry: DeviceRegistry,
    pub drag: Option<ClipDrag>,
    pub time_drag: Option<TimeDrag>,
    pub track_drag: Option<TrackId>,
    pub track_drop: Option<TrackDrop>,
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
    pub clip_editor: Option<ClipEditorState>,
    pub clip_editor_content_rect: Rect,
    pub edit_focus: EditFocus,
    pub plugin_search: BTreeMap<notwig_core::DeviceId, String>,
    pub last_touched_param: BTreeMap<notwig_core::DeviceId, notwig_core::ParamId>,
    pub collapsed_devices: BTreeSet<notwig_core::DeviceId>,
    pub device_browser: Option<DeviceBrowserState>,
    pub show_info: bool,
    pub device_panel_open: bool,
    device_panel_was_open: bool,
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
            track_drop: None,
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
            clip_editor: None,
            clip_editor_content_rect: Rect::NOTHING,
            edit_focus: EditFocus::Timeline,
            plugin_search: BTreeMap::new(),
            last_touched_param: BTreeMap::new(),
            collapsed_devices: BTreeSet::new(),
            device_browser: None,
            show_info: true,
            device_panel_open: true,
            device_panel_was_open: true,
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
        if self.clip_editor.is_some() {
            self.clip_editor_panel(ctx, doc, runtime);
        } else {
            self.device_panel(ctx, doc, runtime);
        }
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

    pub(crate) fn open_clip_editor(&mut self, clip_id: notwig_core::ClipId) {
        if self.clip_editor.is_none() {
            self.device_panel_was_open = self.device_panel_open;
        }
        self.device_panel_open = false;
        self.clip_editor = Some(ClipEditorState::new(clip_id));
        self.edit_focus = EditFocus::ClipEditor;
    }

    pub(crate) fn close_clip_editor(&mut self) {
        self.clip_editor = None;
        self.device_panel_open = self.device_panel_was_open;
        self.edit_focus = EditFocus::Timeline;
    }

    fn selected_audio_clip(&self, doc: &ProjectDoc) -> Option<notwig_core::ClipId> {
        self.state
            .selection
            .primary_clip()
            .filter(|clip_id| {
                doc.arranger
                    .clips
                    .get(clip_id)
                    .is_some_and(|clip| matches!(clip.payload, notwig_core::ClipPayload::Audio(_)))
            })
            .or_else(|| {
                self.state.selection.clips.iter().copied().find(|clip_id| {
                    doc.arranger.clips.get(clip_id).is_some_and(|clip| {
                        matches!(clip.payload, notwig_core::ClipPayload::Audio(_))
                    })
                })
            })
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
        let clip_editor_focused =
            self.edit_focus == EditFocus::ClipEditor && self.clip_editor.is_some();
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
        let target = EditTarget::active(clips.len(), time.as_ref().map(|(range, _)| *range));
        let copy_time = if target == Some(EditTarget::Bar) {
            time.clone()
        } else {
            None
        };
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
        if copy_event && !clip_editor_focused {
            ctx.copy_text("notwig-clipboard".into());
            self.emit(UiIntent::CopySelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if cut_event && !clip_editor_focused {
            ctx.copy_text("notwig-clipboard".into());
            self.emit(UiIntent::CutSelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if paste_event && !clip_editor_focused {
            let at = paste_target(time.as_ref().map(|(range, _)| *range), runtime.playhead);
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
            if ctx.input(|i| i.key_pressed(egui::Key::G)) {
                let tracks: Vec<_> = self.state.selection.tracks.iter().copied().collect();
                if !tracks.is_empty() {
                    self.emit(UiIntent::GroupTracks { tracks });
                }
            }
            if !clip_editor_focused && ctx.input(|i| i.key_pressed(egui::Key::D)) {
                match target {
                    Some(EditTarget::Bar) => {
                        if let Some((range, tracks)) = time.clone() {
                            self.emit(UiIntent::DuplicateTimeSelection {
                                range,
                                tracks: tracks.clone(),
                            });
                            if let Some(next) = duplicate_bar_range(range) {
                                self.state.selection.select_time(next, &tracks);
                            }
                        }
                    }
                    Some(EditTarget::Nodes) => {
                        let spans: Vec<DuplicateSpan<notwig_core::ClipId>> = clips
                            .iter()
                            .filter_map(|clip_id| {
                                doc.arranger.clips.get(clip_id).map(|clip| DuplicateSpan {
                                    id: *clip_id,
                                    range: clip.range,
                                })
                            })
                            .collect();
                        let plan = duplicate_plan(&spans);
                        let mut duplicates = Vec::new();
                        let mut selection = Vec::new();
                        for span in plan {
                            let Some(clip) = doc.arranger.clips.get(&span.id) else {
                                continue;
                            };
                            let new_id = notwig_core::ClipId::from_uuid(uuid::Uuid::new_v4());
                            duplicates.push(ClipDuplicate {
                                old_id: span.id,
                                new_id,
                                range: span.range,
                            });
                            selection.push((new_id, clip.track_id));
                        }
                        if !duplicates.is_empty() {
                            self.state.selection.select_clips(&selection);
                            self.emit(UiIntent::DuplicateClips { clips: duplicates });
                        }
                    }
                    None => {
                        if let Some(track) = self
                            .state
                            .selection
                            .tracks
                            .primary()
                            .or_else(|| self.state.selection.tracks.iter().next().copied())
                        {
                            self.emit(UiIntent::DuplicateTrack(track));
                        }
                    }
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num0)) {
                self.zoom_to_fit(doc);
            }
        }
        if escape {
            if clip_editor_focused {
                self.close_clip_editor();
            } else {
                self.state.selection.clear();
            }
        }
        if delete && !clip_editor_focused {
            let time = self
                .state
                .selection
                .time
                .as_ref()
                .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
            if target == Some(EditTarget::Bar)
                && let Some((range, tracks)) = time
            {
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
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 42.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        self.file_menu(ui);
                        ui.add_space(4.0);
                        let play_icon = if runtime.playing {
                            Icon::Pause
                        } else {
                            Icon::Play
                        };
                        if icon_button_sized(
                            ui,
                            play_icon,
                            egui::vec2(42.0, 42.0),
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
                        if icon_button_sized(ui, Icon::Stop, egui::vec2(42.0, 42.0), false, "Stop")
                            .clicked()
                        {
                            self.emit(UiIntent::Stop);
                        }
                        record_button(ui);
                        if icon_button_sized(
                            ui,
                            Icon::Loop,
                            egui::vec2(42.0, 42.0),
                            doc.transport.loop_enabled,
                            "Toggle loop",
                        )
                        .clicked()
                        {
                            self.emit(UiIntent::ToggleLoop);
                        }
                        let engine_width = 400.0;
                        let total_width = ui.max_rect().width();
                        let left_used = ui.cursor().left() - ui.max_rect().left();
                        ui.add_space(((total_width - engine_width) * 0.5 - left_used).max(0.0));
                        ui.allocate_ui_with_layout(
                            egui::vec2(engine_width, 42.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(16, 17, 21))
                                    .corner_radius(egui::CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(engine_width - 16.0, 34.0));
                                        ui.horizontal(|ui| {
                                            cpu_component(ui, runtime.cpu_load.unwrap_or_default());
                                            ui.separator();
                                            let mut tempo = doc.meta.tempo_bpm;
                                            ui.scope(|ui| {
                                                ui.visuals_mut().override_text_color =
                                                    Some(Color32::from_rgb(104, 164, 236));
                                                ui.style_mut().text_styles.insert(
                                                    egui::TextStyle::Button,
                                                    egui::FontId::monospace(15.0),
                                                );
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut tempo)
                                                            .range(20.0..=400.0)
                                                            .speed(0.1)
                                                            .fixed_decimals(2),
                                                    )
                                                    .on_hover_text("Tempo (BPM)")
                                                    .changed()
                                                {
                                                    self.emit(UiIntent::SetTempo(tempo));
                                                }
                                            });
                                            ui.separator();
                                            let mut numerator = doc.meta.time_sig_num;
                                            ui.scope(|ui| {
                                                ui.visuals_mut().override_text_color =
                                                    Some(Color32::from_rgb(104, 164, 236));
                                                ui.style_mut().text_styles.insert(
                                                    egui::TextStyle::Button,
                                                    egui::FontId::monospace(15.0),
                                                );
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut numerator)
                                                            .range(1..=32),
                                                    )
                                                    .changed()
                                                {
                                                    self.emit(UiIntent::SetTimeSignature {
                                                        numerator,
                                                        denominator: doc.meta.time_sig_den,
                                                    });
                                                }
                                            });
                                            ui.label(
                                                RichText::new("/")
                                                    .size(15.0)
                                                    .monospace()
                                                    .color(theme::TEXT_MUTED),
                                            );
                                            let mut denominator = doc.meta.time_sig_den;
                                            egui::ComboBox::from_id_salt("time-signature-den")
                                                .selected_text(
                                                    RichText::new(denominator.to_string())
                                                        .monospace()
                                                        .size(15.0)
                                                        .color(Color32::from_rgb(104, 164, 236)),
                                                )
                                                .width(32.0)
                                                .show_ui(ui, |ui| {
                                                    for value in [1u8, 2, 4, 8, 16, 32] {
                                                        if ui
                                                            .selectable_value(
                                                                &mut denominator,
                                                                value,
                                                                value.to_string(),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.emit(UiIntent::SetTimeSignature {
                                                                numerator: doc.meta.time_sig_num,
                                                                denominator: value,
                                                            });
                                                        }
                                                    }
                                                });
                                            ui.separator();
                                            ui.scope(|ui| {
                                                ui.spacing_mut().item_spacing.y = 0.0;
                                                ui.vertical(|ui| {
                                                    let blue = Color32::from_rgb(104, 164, 236);
                                                    ui.label(
                                                        RichText::new(format_position_bars(
                                                            runtime.playhead,
                                                            doc.meta.time_sig_num,
                                                        ))
                                                        .monospace()
                                                        .size(13.0)
                                                        .color(blue),
                                                    );
                                                    ui.label(
                                                        RichText::new(format_position_time(
                                                            runtime.playhead,
                                                            doc.meta.tempo_bpm,
                                                        ))
                                                        .monospace()
                                                        .size(13.0)
                                                        .color(blue),
                                                    );
                                                });
                                            });
                                            ui.separator();
                                            let master = doc
                                                .master_track()
                                                .and_then(|id| runtime.meters.get(&id));
                                            master_meter(ui, master);
                                        });
                                    });
                            },
                        );
                    },
                );
            });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(76.0, 42.0), egui::Sense::click());
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(4),
            if response.hovered() {
                theme::GRID_STRONG
            } else {
                theme::PANEL
            },
        );
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0_f32, theme::GRID),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "FILE",
            egui::FontId::monospace(11.0),
            Color32::WHITE,
        );
        egui::Popup::menu(&response).show(|ui| {
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

    fn status_bar(&mut self, ctx: &egui::Context, doc: &ProjectDoc, _runtime: &RuntimeView) {
        egui::TopBottomPanel::bottom("notwig_status")
            .exact_height(40.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_LIGHT)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 32.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if footer_icon(ui, FooterIcon::Info, self.show_info, "Selection info")
                            .clicked()
                        {
                            self.show_info = !self.show_info;
                        }
                        ui.separator();
                        footer_text(ui, "ARRANGE", theme::ACCENT);
                        footer_text(ui, "MIX", theme::TEXT_MUTED);
                        footer_text(ui, "EDIT", theme::TEXT_MUTED);
                        ui.separator();
                        footer_icon(ui, FooterIcon::Grid, false, "Grid");
                        footer_icon(ui, FooterIcon::Share, false, "Share");
                        footer_icon(ui, FooterIcon::Monitor, false, "Monitor");
                        if footer_icon(
                            ui,
                            FooterIcon::Sliders,
                            self.device_panel_open && self.clip_editor.is_none(),
                            "Toggle device panel",
                        )
                        .clicked()
                        {
                            self.close_clip_editor();
                            self.device_panel_open = !self.device_panel_open;
                        }
                        let selected_clip = self.selected_audio_clip(doc);
                        if footer_icon(
                            ui,
                            FooterIcon::Clip,
                            self.clip_editor.is_some(),
                            "Toggle clip editor",
                        )
                        .clicked()
                        {
                            if self.clip_editor.is_some() {
                                self.close_clip_editor();
                            } else if let Some(clip_id) = selected_clip {
                                self.open_clip_editor(clip_id);
                            }
                        }
                        ui.add_space(12.0);
                        let action = if self.device_panel_open {
                            "Hide Device Panel"
                        } else {
                            "Show Device Panel"
                        };
                        let toggle = footer_action(ui, action);
                        if toggle {
                            self.device_panel_open = !self.device_panel_open;
                        }
                    },
                );
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

    fn clip_editor_drop_point(
        &self,
        doc: &ProjectDoc,
        pointer: egui::Pos2,
    ) -> Option<(notwig_core::ClipId, notwig_core::TimeMus)> {
        let state = self.clip_editor.as_ref()?;
        if !self.clip_editor_content_rect.contains(pointer) {
            return None;
        }
        let clip = doc.arranger.clips.get(&state.clip_id)?;
        let ppb = state.viewport.pixels_per_beat.clamp(
            crate::state::Viewport::MIN_PPB,
            crate::state::Viewport::MAX_PPB,
        );
        let grid = crate::widgets::snap_step(ppb, doc.meta.time_sig_num);
        let absolute = crate::widgets::snap_time(
            state
                .viewport
                .x_to_beat(pointer.x, self.clip_editor_content_rect.left()),
            grid,
            false,
        );
        let at = (absolute - clip.range.start).max(notwig_core::TimeMus::zero());
        Some((state.clip_id, at))
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
        let clip_drop = self.clip_editor_drop_point(doc, pointer);
        let (target, at) = if let Some((clip_id, at)) = clip_drop {
            (DropAudioTarget::ClipEditor { clip_id }, at)
        } else {
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
            let grid = crate::widgets::snap_step(
                self.state.viewport.pixels_per_beat,
                doc.meta.time_sig_num,
            );
            let at = crate::widgets::snap_time(
                self.state.viewport.x_to_beat(pointer.x, self.timeline_left),
                grid,
                false,
            );
            (target, at)
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
        self.drag_preview = Some(DragPreview {
            path,
            target,
            at,
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
            if let Some((clip_id, at)) = self.clip_editor_drop_point(doc, pointer) {
                return Some((DropAudioTarget::ClipEditor { clip_id }, at));
            }
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
                "Drop audio files onto the timeline or clip editor.",
            );
            return;
        };
        let order: Vec<TrackId> = doc.track_order.depth_first();
        for (file_index, file) in files.into_iter().enumerate() {
            let target = match (file_index, first_target) {
                (0, target) | (_, target @ DropAudioTarget::ClipEditor { .. }) => target,
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

#[derive(Clone, Copy)]
enum FooterIcon {
    Info,
    Grid,
    Share,
    Monitor,
    Sliders,
    Clip,
}

fn footer_icon(
    ui: &mut egui::Ui,
    icon: FooterIcon,
    selected: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(3),
            if selected {
                theme::ACCENT_DIM
            } else {
                theme::GRID_STRONG
            },
        );
    }
    let color = if selected {
        theme::ACCENT
    } else {
        theme::TEXT_MUTED
    };
    let c = rect.center();
    match icon {
        FooterIcon::Info => {
            painter.circle_stroke(c, 6.0, egui::Stroke::new(1.2_f32, color));
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::monospace(10.0),
                color,
            );
        }
        FooterIcon::Grid => {
            for row in 0..3 {
                for col in 0..3 {
                    painter.circle_filled(
                        egui::pos2(c.x - 6.0 + col as f32 * 6.0, c.y - 6.0 + row as f32 * 6.0),
                        1.4,
                        color,
                    );
                }
            }
        }
        FooterIcon::Share => {
            painter.circle_stroke(
                egui::pos2(c.x - 6.0, c.y),
                3.0,
                egui::Stroke::new(1.2_f32, color),
            );
            painter.circle_stroke(
                egui::pos2(c.x + 6.0, c.y - 4.0),
                3.0,
                egui::Stroke::new(1.2_f32, color),
            );
            painter.circle_stroke(
                egui::pos2(c.x + 6.0, c.y + 4.0),
                3.0,
                egui::Stroke::new(1.2_f32, color),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.2, c.y - 1.0),
                    egui::pos2(c.x + 3.2, c.y - 3.3),
                ],
                egui::Stroke::new(1.0_f32, color),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.2, c.y + 1.0),
                    egui::pos2(c.x + 3.2, c.y + 3.3),
                ],
                egui::Stroke::new(1.0_f32, color),
            );
        }
        FooterIcon::Monitor => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(15.0, 11.0)),
                egui::CornerRadius::same(1),
                egui::Stroke::new(1.2_f32, color),
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 4.0, c.y + 8.0),
                    egui::pos2(c.x + 4.0, c.y + 8.0),
                ],
                egui::Stroke::new(1.2_f32, color),
            );
        }
        FooterIcon::Clip => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(16.0, 12.0)),
                egui::CornerRadius::same(2),
                egui::Stroke::new(1.2_f32, color),
                egui::StrokeKind::Inside,
            );
            for offset in [-4.0_f32, 0.0, 4.0] {
                painter.line_segment(
                    [
                        egui::pos2(c.x + offset, c.y - 4.0),
                        egui::pos2(c.x + offset, c.y + 4.0),
                    ],
                    egui::Stroke::new(1.0_f32, color),
                );
            }
        }
        FooterIcon::Sliders => {
            for offset in [-6.0_f32, 0.0, 6.0] {
                painter.line_segment(
                    [
                        egui::pos2(c.x + offset, c.y - 7.0),
                        egui::pos2(c.x + offset, c.y + 7.0),
                    ],
                    egui::Stroke::new(1.2_f32, color),
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x + offset - 2.5, c.y + offset * 0.4),
                        egui::pos2(c.x + offset + 2.5, c.y + offset * 0.4),
                    ],
                    egui::Stroke::new(2.0_f32, color),
                );
            }
        }
    }
    response.on_hover_text(tooltip)
}

fn footer_text(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(
        RichText::new(text)
            .monospace()
            .size(11.5)
            .strong()
            .color(color),
    );
}

fn footer_action(ui: &mut egui::Ui, action: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new(RichText::new("CLICK").strong()).frame(false))
            .clicked()
        {
            clicked = true;
        }
        if ui
            .add(egui::Button::new(RichText::new(action).color(theme::TEXT_MUTED)).frame(false))
            .clicked()
        {
            clicked = true;
        }
    });
    clicked
}

pub(crate) fn cpu_component(ui: &mut egui::Ui, cpu: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(60.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    let icon = Rect::from_center_size(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        egui::Vec2::splat(10.0),
    );
    let blue = Color32::from_rgb(80, 145, 230);
    painter.rect_stroke(
        icon,
        egui::CornerRadius::same(1),
        egui::Stroke::new(1.0_f32, blue),
        egui::StrokeKind::Inside,
    );
    for offset in [-3.0_f32, 0.0, 3.0] {
        painter.line_segment(
            [
                egui::pos2(icon.left() - 2.0, icon.center().y + offset),
                egui::pos2(icon.left(), icon.center().y + offset),
            ],
            egui::Stroke::new(1.0_f32, blue),
        );
        painter.line_segment(
            [
                egui::pos2(icon.right(), icon.center().y + offset),
                egui::pos2(icon.right() + 2.0, icon.center().y + offset),
            ],
            egui::Stroke::new(1.0_f32, blue),
        );
    }
    let bar = Rect::from_min_max(
        egui::pos2(rect.left() + 21.0, rect.center().y - 2.5),
        egui::pos2(rect.right() - 2.0, rect.center().y + 2.5),
    );
    painter.rect_filled(bar, egui::CornerRadius::ZERO, theme::GRID);
    let fill = Rect::from_min_max(
        bar.min,
        egui::pos2(
            bar.left() + bar.width() * (cpu / 100.0).clamp(0.0, 1.0),
            bar.bottom(),
        ),
    );
    painter.rect_filled(fill, egui::CornerRadius::ZERO, blue);
}

fn record_button(ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(4),
        if response.hovered() {
            theme::GRID_STRONG
        } else {
            theme::PANEL
        },
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(1.0_f32, theme::GRID),
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(rect.center(), 9.0, Color32::from_rgb(231, 91, 82));
    response.on_hover_text("Record (not implemented)");
}

fn format_position_bars(time: notwig_core::TimeMus, beats_per_bar: u8) -> String {
    let beats = time.as_f64().max(0.0);
    let bar_len = f64::from(beats_per_bar.max(1));
    let bar = (beats / bar_len).floor() as u64 + 1;
    let beat_pos = beats % bar_len;
    let beat = beat_pos.floor() as u64 + 1;
    let sixteenth_pos = beat_pos.fract() * 4.0;
    let sixteenth = sixteenth_pos.floor() as u64 + 1;
    let tick = (sixteenth_pos.fract() * 100.0).floor() as u64;
    format!("{bar}.{beat}.{sixteenth}.{tick:02}")
}

fn format_position_time(time: notwig_core::TimeMus, tempo_bpm: f64) -> String {
    let seconds = (time.as_f64() * 60.0 / tempo_bpm.max(1.0)).max(0.0);
    let minutes = (seconds / 60.0).floor() as u64;
    let seconds = seconds % 60.0;
    format!("{minutes}:{seconds:06.3}")
}

fn master_meter(ui: &mut egui::Ui, meter: Option<&crate::runtime::MeterValues>) {
    let meter = meter.copied().unwrap_or_default();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 32.0), egui::Sense::hover());
    crate::widgets::draw_stereo_meter(ui.painter(), rect, meter.peak_l, meter.peak_r);
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

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn clip_editor_doc() -> ProjectDoc {
        let meta = notwig_core::Meta {
            name: "Shell".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: notwig_core::SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        ProjectDoc::new_template(
            meta,
            notwig_core::TrackId::nil(),
            notwig_core::DeviceChainId::nil(),
        )
        .unwrap()
    }

    fn timeline_doc() -> ProjectDoc {
        let meta = notwig_core::Meta {
            name: "Timeline".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: notwig_core::SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        let mut doc = ProjectDoc::new_template(
            meta,
            notwig_core::TrackId::nil(),
            notwig_core::DeviceChainId::nil(),
        )
        .unwrap();
        let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
        let chain = notwig_core::DeviceChainId::from_uuid(uuid::Uuid::from_u128(102));
        doc = notwig_core::apply(
            &doc,
            &notwig_core::protocol::Command::new(
                notwig_core::protocol::CommandPayload::CreateTrack {
                    id: track,
                    chain_id: chain,
                    kind: TrackKind::Instrument,
                    name: "Keys".into(),
                    parent: None,
                    index: None,
                    color: None,
                },
            ),
        )
        .unwrap()
        .doc;
        notwig_core::apply(
            &doc,
            &notwig_core::protocol::Command::new(
                notwig_core::protocol::CommandPayload::CreateClip {
                    id: notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20)),
                    track_id: track,
                    kind: notwig_core::ClipKind::Midi,
                    range: notwig_core::RangeMus::new(
                        notwig_core::Rational::zero(),
                        notwig_core::Rational::new(2, 1),
                    )
                    .unwrap(),
                    payload: notwig_core::ClipPayload::Midi(notwig_core::sequencer::MidiClipData {
                        length: notwig_core::Rational::new(2, 1),
                        content_start: notwig_core::Rational::zero(),
                        notes: Vec::new(),
                    }),
                },
            ),
        )
        .unwrap()
        .doc
    }

    #[test]
    fn timeline_duplicate_advances_node_selection() {
        let doc = timeline_doc();
        let clip = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20));
        let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
        let runtime = RuntimeView::default();
        let ctx = egui::Context::default();
        let mut ui = NotwigUi::default();
        ui.state.selection.select_clip(clip, track, false);
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
                events: vec![egui::Event::Key {
                    key: egui::Key::D,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            },
            |ctx| {
                ui.handle_shortcuts(ctx, &doc, &runtime);
            },
        );
        assert_eq!(ui.state.selection.clips.len(), 1);
        assert!(!ui.state.selection.clips.contains(&clip));
        assert!(
            ui.intents
                .iter()
                .any(|intent| matches!(intent, UiIntent::DuplicateClips { .. }))
        );
    }

    fn shortcut_intents(
        doc: &ProjectDoc,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> Vec<UiIntent> {
        let runtime = RuntimeView::default();
        let ctx = egui::Context::default();
        let (clip, track) = doc.arranger.clips.iter().next().map_or(
            (notwig_core::ClipId::nil(), notwig_core::TrackId::nil()),
            |(id, clip)| (*id, clip.track_id),
        );
        let mut ui = NotwigUi::default();
        ui.state.selection.select_clip(clip, track, false);
        ui.clip_editor = Some(ClipEditorState::new(clip));
        ui.edit_focus = EditFocus::Timeline;
        let mut intents = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                ui.handle_shortcuts(ctx, doc, &runtime);
                intents = std::mem::take(&mut ui.intents);
            },
        );
        intents
    }

    #[test]
    fn timeline_shortcut_family_still_runs_while_clip_editor_is_open() {
        let doc = timeline_doc();
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let copy = shortcut_intents(&doc, vec![egui::Event::Copy], command);
        assert!(matches!(copy.as_slice(), [UiIntent::CopySelection { .. }]));
        let cut = shortcut_intents(&doc, vec![egui::Event::Cut], command);
        assert!(matches!(cut.as_slice(), [UiIntent::CutSelection { .. }]));
        let paste = shortcut_intents(&doc, vec![egui::Event::Paste(String::new())], command);
        assert!(matches!(paste.as_slice(), [UiIntent::PasteClips { .. }]));
        let duplicate = shortcut_intents(
            &doc,
            vec![egui::Event::Key {
                key: egui::Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            }],
            command,
        );
        assert!(
            duplicate
                .iter()
                .any(|intent| matches!(intent, UiIntent::DuplicateClips { .. }))
        );
        let delete = shortcut_intents(
            &doc,
            vec![egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }],
            egui::Modifiers::default(),
        );
        assert!(
            delete
                .iter()
                .any(|intent| matches!(intent, UiIntent::DeleteClip(_)))
        );
    }

    #[test]
    fn timeline_shortcuts_still_run_while_clip_editor_is_open() {
        let doc = clip_editor_doc();
        let clip = notwig_core::ClipId::nil();
        let track = notwig_core::TrackId::nil();
        let runtime = RuntimeView::default();
        let ctx = egui::Context::default();
        let mut ui = NotwigUi::default();
        ui.state.selection.select_clip(clip, track, false);
        ui.clip_editor = Some(ClipEditorState::new(clip));

        let mut intents = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ctx| {
                ui.handle_shortcuts(ctx, &doc, &runtime);
                intents = std::mem::take(&mut ui.intents);
            },
        );
        assert!(intents.iter().any(|intent| matches!(
            intent,
            UiIntent::CopySelection { clips, .. } if clips.contains(&clip)
        )));
    }

    #[test]
    fn clip_editor_and_device_panel_are_mutually_exclusive() {
        let mut ui = NotwigUi {
            device_panel_open: true,
            ..NotwigUi::default()
        };
        ui.open_clip_editor(notwig_core::ClipId::nil());
        assert!(ui.clip_editor.is_some());
        assert!(!ui.device_panel_open);
        ui.close_clip_editor();
        assert!(ui.clip_editor.is_none());
        assert!(ui.device_panel_open);
    }
}
