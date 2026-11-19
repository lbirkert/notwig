use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use egui::{Color32, Rect, RichText};
use notwig_core::{ProjectDoc, TrackId, TrackKind};
use notwig_dsp::DeviceRegistry;

use crate::arranger::{BarSelectionDrag, ClipDrag, GroupDrag, TimeDrag, TrackDrop};
use crate::commands::{COMMANDS, CommandCategory, KeyChord, ShortcutCommand, Shortcuts};
use crate::drop_target::DragPreview;
use crate::icons::{Icon, icon_button_sized};
use crate::intent::{DropAudioTarget, UiIntent};
use crate::runtime::RuntimeView;
use crate::state::{EditFocus, NotificationKind, OpenedEditor, UiState};
use crate::theme;
use crate::views::audio_editor::ClipEditorState;
use crate::views::device_editor::{DeviceBrowserState, EqDrag};
use crate::views::piano_roll::PianoRollState;
use notwig_core::edit::SelectionMode;

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
    pub piano_roll: Option<PianoRollState>,
    pub piano_roll_content_rect: Rect,
    pub edit_focus: EditFocus,
    pub plugin_search: BTreeMap<notwig_core::DeviceId, String>,
    pub last_touched_param: BTreeMap<notwig_core::DeviceId, notwig_core::ParamId>,
    pub collapsed_devices: BTreeSet<notwig_core::DeviceId>,
    pub device_browser: Option<DeviceBrowserState>,
    pub show_info: bool,
    pub currently_opened: Option<OpenedEditor>,
    pub settings_open: bool,
    pub settings_category: crate::settings::SettingsCategory,
    pub audio_draft: crate::runtime::AudioSettingsValue,
    pub(crate) shortcuts: Shortcuts,
    pub(crate) shortcut_editor: crate::commands::ShortcutEditorState,
    pub dsp_open: bool,
    pub render_open: bool,
    pub render_start: notwig_core::TimeMus,
    pub render_end: notwig_core::TimeMus,
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
            piano_roll: None,
            piano_roll_content_rect: Rect::NOTHING,
            edit_focus: EditFocus::Timeline,
            plugin_search: BTreeMap::new(),
            last_touched_param: BTreeMap::new(),
            collapsed_devices: BTreeSet::new(),
            device_browser: None,
            show_info: true,
            currently_opened: Some(OpenedEditor::DeviceEditor),
            settings_open: false,
            settings_category: crate::settings::SettingsCategory::Audio,
            audio_draft: crate::runtime::AudioSettingsValue::default(),
            shortcuts: Shortcuts::default(),
            shortcut_editor: crate::commands::ShortcutEditorState::default(),
            dsp_open: false,
            render_open: false,
            render_start: notwig_core::TimeMus::zero(),
            render_end: notwig_core::TimeMus::zero(),
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
        match self.currently_opened {
            Some(OpenedEditor::ClipEditor) => self.clip_editor_panel(ctx, doc, runtime),
            Some(OpenedEditor::PianoRoll) => self.piano_roll_panel(ctx, doc, runtime),
            Some(OpenedEditor::DeviceEditor) => self.device_panel(ctx, doc, runtime),
            None => {}
        }
        self.device_browser_window(ctx, doc, runtime);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| self.arranger_view(ui, doc, runtime));
        if self.settings_open {
            self.settings_window(ctx, runtime);
        }
        if self.dsp_open {
            self.dsp_window(ctx, runtime);
        }
        if self.render_open {
            self.render_window(ctx, doc);
        }
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

    #[must_use]
    pub fn shortcuts(&self) -> &Shortcuts {
        &self.shortcuts
    }

    pub fn set_shortcuts(&mut self, shortcuts: Shortcuts) {
        self.shortcuts = shortcuts;
    }

    #[must_use]
    pub fn take_shortcuts_dirty(&mut self) -> bool {
        self.shortcuts.take_dirty()
    }

    pub(crate) fn emit(&mut self, intent: UiIntent) {
        if !self.intents.contains(&intent) {
            self.intents.push(intent);
        }
    }

    pub fn open_clip_editor(&mut self, clip_id: notwig_core::ClipId) {
        self.currently_opened = Some(OpenedEditor::ClipEditor);
        self.clip_editor = Some(ClipEditorState::new(clip_id));
        self.piano_roll = None;
        self.edit_focus = EditFocus::ClipEditor;
    }

    pub fn open_piano_roll(&mut self, clip_id: notwig_core::ClipId) {
        self.currently_opened = Some(OpenedEditor::PianoRoll);
        self.piano_roll = Some(PianoRollState::new(clip_id));
        self.clip_editor = None;
        self.edit_focus = EditFocus::PianoRoll;
    }

    pub fn open_editor_for_clip(&mut self, doc: &ProjectDoc, clip_id: notwig_core::ClipId) {
        let Some(clip) = doc.arranger.clips.get(&clip_id) else {
            return;
        };
        self.state
            .selection
            .select_clip(clip_id, clip.track_id, false);
        match &clip.payload {
            notwig_core::ClipPayload::Midi(_) => self.open_piano_roll(clip_id),
            notwig_core::ClipPayload::Audio(_) => self.open_clip_editor(clip_id),
        }
    }

    pub fn close_clip_editor(&mut self) {
        if matches!(
            self.currently_opened,
            Some(OpenedEditor::ClipEditor | OpenedEditor::PianoRoll)
        ) {
            self.currently_opened = None;
        }
        self.clip_editor = None;
        self.piano_roll = None;
        self.edit_focus = EditFocus::Timeline;
    }

    pub fn close_piano_roll(&mut self) {
        self.close_clip_editor();
    }

    pub fn open_device_editor(&mut self) {
        self.currently_opened = Some(OpenedEditor::DeviceEditor);
        self.clip_editor = None;
        self.piano_roll = None;
        self.edit_focus = EditFocus::Timeline;
    }

    pub fn close_editor(&mut self) {
        self.currently_opened = None;
        self.clip_editor = None;
        self.piano_roll = None;
        self.edit_focus = EditFocus::Timeline;
    }

    pub fn toggle_device_editor(&mut self) {
        if self.currently_opened == Some(OpenedEditor::DeviceEditor) {
            self.close_editor();
        } else {
            self.open_device_editor();
        }
    }

    pub fn toggle_clip_editor(&mut self, doc: &ProjectDoc) {
        if matches!(
            self.currently_opened,
            Some(OpenedEditor::ClipEditor | OpenedEditor::PianoRoll)
        ) {
            self.close_clip_editor();
            return;
        }
        if let Some(clip_id) = self
            .state
            .selection
            .primary_clip()
            .filter(|clip_id| doc.arranger.clips.contains_key(clip_id))
        {
            self.open_editor_for_clip(doc, clip_id);
            return;
        }
        if let Some(clip_id) = self.selected_midi_clip(doc) {
            self.open_piano_roll(clip_id);
        } else if let Some(clip_id) = self.selected_audio_clip(doc) {
            self.open_clip_editor(clip_id);
        } else {
            self.currently_opened = Some(OpenedEditor::ClipEditor);
            self.clip_editor = None;
            self.piano_roll = None;
            self.edit_focus = EditFocus::ClipEditor;
        }
    }

    fn selected_audio_clip(&self, doc: &ProjectDoc) -> Option<notwig_core::ClipId> {
        self.selected_clip_of_kind(doc, |payload| {
            matches!(payload, notwig_core::ClipPayload::Audio(_))
        })
    }

    fn selected_midi_clip(&self, doc: &ProjectDoc) -> Option<notwig_core::ClipId> {
        self.selected_clip_of_kind(doc, |payload| {
            matches!(payload, notwig_core::ClipPayload::Midi(_))
        })
    }

    fn selected_clip_of_kind(
        &self,
        doc: &ProjectDoc,
        matches_kind: impl Fn(&notwig_core::ClipPayload) -> bool,
    ) -> Option<notwig_core::ClipId> {
        self.state
            .selection
            .primary_clip()
            .filter(|clip_id| {
                doc.arranger
                    .clips
                    .get(clip_id)
                    .is_some_and(|clip| matches_kind(&clip.payload))
            })
            .or_else(|| {
                self.state.selection.clips.iter().copied().find(|clip_id| {
                    doc.arranger
                        .clips
                        .get(clip_id)
                        .is_some_and(|clip| matches_kind(&clip.payload))
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
        match self.currently_opened {
            Some(OpenedEditor::ClipEditor) => {
                let selected = self.selected_audio_clip(doc);
                match (
                    selected,
                    self.clip_editor.as_ref().map(|state| state.clip_id),
                ) {
                    (Some(id), Some(current)) if current == id => {}
                    (Some(id), _) => self.clip_editor = Some(ClipEditorState::new(id)),
                    (None, _) => self.clip_editor = None,
                }
            }
            Some(OpenedEditor::PianoRoll) => {
                let selected = self.selected_midi_clip(doc);
                match (
                    selected,
                    self.piano_roll.as_ref().map(|state| state.clip_id),
                ) {
                    (Some(id), Some(current)) if current == id => {}
                    (Some(id), _) => self.piano_roll = Some(PianoRollState::new(id)),
                    (None, _) => self.piano_roll = None,
                }
            }
            _ => {}
        }
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
        if self.shortcut_editor.capture.is_some() {
            return;
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let clip_editor_focused = self.edit_focus == EditFocus::ClipEditor
            && self.currently_opened == Some(OpenedEditor::ClipEditor)
            && self.clip_editor.is_some();
        let piano_roll_focused = self.edit_focus == EditFocus::PianoRoll
            && self.currently_opened == Some(OpenedEditor::PianoRoll)
            && self.piano_roll.is_some();
        let editor_focused = clip_editor_focused || piano_roll_focused;
        let time = self
            .state
            .selection
            .time
            .as_ref()
            .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
        let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
        let target = self.state.selection.to_edit_selection().mode();
        let copy_time = if target == Some(SelectionMode::Time) {
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
        let copy = self.shortcuts.was_pressed(ctx, ShortcutCommand::Copy)
            || (copy_event
                && self
                    .shortcuts
                    .has_binding(ShortcutCommand::Copy, KeyChord::command(egui::Key::C)));
        let cut = self.shortcuts.was_pressed(ctx, ShortcutCommand::Cut)
            || (cut_event
                && self
                    .shortcuts
                    .has_binding(ShortcutCommand::Cut, KeyChord::command(egui::Key::X)));
        let paste = self.shortcuts.was_pressed(ctx, ShortcutCommand::Paste)
            || (paste_event
                && self
                    .shortcuts
                    .has_binding(ShortcutCommand::Paste, KeyChord::command(egui::Key::V)));
        if copy && !editor_focused {
            ctx.copy_text("notwig-clipboard".into());
            self.emit(UiIntent::CopySelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if cut && !editor_focused {
            ctx.copy_text("notwig-clipboard".into());
            self.emit(UiIntent::CutSelection {
                clips: clips.clone(),
                time: copy_time.clone(),
            });
        }
        if paste && !editor_focused {
            let at = time
                .as_ref()
                .map_or(runtime.playhead, |(range, _)| range.start);
            self.emit(UiIntent::PasteClips { at });
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Stop) {
            self.emit(UiIntent::Stop);
        } else if self.shortcuts.was_pressed(ctx, ShortcutCommand::Play) {
            if doc.transport.playing {
                self.emit(UiIntent::Stop);
            } else {
                self.emit(UiIntent::Play);
            }
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::SaveAs) {
            self.emit(UiIntent::SaveAs);
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Save) {
            self.emit(UiIntent::Save);
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::NewProject) {
            self.emit(UiIntent::NewProject);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::OpenProject)
        {
            self.emit(UiIntent::OpenProject);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::OpenSettings)
            || self
                .shortcuts
                .was_pressed(ctx, ShortcutCommand::AudioSettings)
        {
            self.open_settings(runtime);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::LoopSelection)
        {
            if let Some((range, _)) = time.clone() {
                self.emit(UiIntent::SetLoopRange(range));
            } else {
                self.emit(UiIntent::ToggleLoop);
            }
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::ToggleLoop) {
            self.emit(UiIntent::ToggleLoop);
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Undo) {
            self.emit(UiIntent::Undo);
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Redo) {
            self.emit(UiIntent::Redo);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::GroupTracks)
        {
            let tracks: Vec<_> = self.state.selection.tracks.iter().copied().collect();
            if !tracks.is_empty() {
                self.emit(UiIntent::GroupTracks { tracks });
            }
        }
        if !editor_focused && self.shortcuts.was_pressed(ctx, ShortcutCommand::Duplicate) {
            match target {
                Some(SelectionMode::Objects | SelectionMode::Time) => {
                    self.emit(UiIntent::DuplicateSelection {
                        clips: clips.clone(),
                        time: time.clone(),
                    });
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
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::FitArrangement)
        {
            self.zoom_to_fit(doc);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::ImportAudio)
        {
            self.emit(UiIntent::ImportAudio);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::RenderMixdown)
        {
            self.open_render(doc);
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::ToggleInfoPanel)
        {
            self.show_info = !self.show_info;
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::ToggleDeviceEditor)
        {
            self.toggle_device_editor();
        }
        if self
            .shortcuts
            .was_pressed(ctx, ShortcutCommand::ToggleClipEditor)
        {
            self.toggle_clip_editor(doc);
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Cancel)
            && !self.settings_open
            && !self.dsp_open
            && !self.render_open
        {
            if editor_focused {
                self.close_clip_editor();
            } else {
                self.state.selection.clear();
            }
        }
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Delete) && !editor_focused {
            let time = self
                .state
                .selection
                .time
                .as_ref()
                .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
            let devices: Vec<_> = self.state.selection.devices.iter().copied().collect();
            let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
            if !devices.is_empty() {
                for device in devices {
                    self.emit(UiIntent::RemoveDevice(device));
                }
            } else if !clips.is_empty() || time.is_some() {
                self.emit(UiIntent::DeleteSelection { clips, time });
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
        if self.shortcuts.was_pressed(ctx, ShortcutCommand::Quit) {
            self.emit(UiIntent::Quit);
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
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 42.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        self.file_menu(ui, doc, runtime);
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
                                            if cpu_component(
                                                ui,
                                                runtime.cpu_load.unwrap_or_default(),
                                            )
                                            .clicked()
                                            {
                                                self.dsp_open = true;
                                            }
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

    fn file_menu(&mut self, ui: &mut egui::Ui, doc: &ProjectDoc, runtime: &RuntimeView) {
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
            if ui.button("Audio Settings…").clicked() {
                self.open_settings(runtime);
                ui.close();
            }
            if ui.button("Render Mixdown…").clicked() {
                self.open_render(doc);
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
                        if footer_icon(ui, Icon::Info, self.show_info, "Selection info").clicked() {
                            self.show_info = !self.show_info;
                        }
                        ui.separator();
                        if footer_icon(
                            ui,
                            Icon::Sliders,
                            self.currently_opened == Some(OpenedEditor::DeviceEditor),
                            "Toggle device editor",
                        )
                        .clicked()
                        {
                            self.toggle_device_editor();
                        }
                        if footer_icon(
                            ui,
                            Icon::Clip,
                            matches!(
                                self.currently_opened,
                                Some(OpenedEditor::ClipEditor | OpenedEditor::PianoRoll)
                            ),
                            "Toggle clip / MIDI editor",
                        )
                        .clicked()
                        {
                            self.toggle_clip_editor(doc);
                        }
                        ui.add_space(12.0);
                        let (action, tooltip) =
                            if self.currently_opened == Some(OpenedEditor::DeviceEditor) {
                                ("Hide Device Editor", "Close the device editor")
                            } else {
                                ("Show Device Editor", "Open the device editor")
                            };
                        if footer_action(ui, action, tooltip) {
                            self.toggle_device_editor();
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
        let shortcuts = &self.shortcuts;
        egui::Window::new("Keyboard & workflow")
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Command shortcuts reflect your current bindings; customize them in \
                         Settings -> Shortcuts.",
                    )
                    .color(theme::TEXT_MUTED),
                );
                ui.add_space(6.0);
                for category in CommandCategory::ALL {
                    ui.label(
                        RichText::new(category.label())
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    for spec in COMMANDS.iter().filter(|spec| spec.category == category) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(binding_label(shortcuts, spec.command))
                                    .monospace()
                                    .color(theme::ACCENT),
                            );
                            ui.label(RichText::new(spec.label).strong());
                            ui.label(RichText::new(spec.description).color(theme::TEXT_MUTED));
                        });
                    }
                    ui.add_space(4.0);
                }
                ui.separator();
                ui.label(
                    RichText::new("Mouse & workflow")
                        .strong()
                        .color(theme::TEXT_MUTED),
                );
                for (key, action) in [
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

fn binding_label(shortcuts: &Shortcuts, command: ShortcutCommand) -> String {
    let bindings = shortcuts.bindings(command);
    if bindings.is_empty() {
        "Unassigned".to_owned()
    } else {
        bindings
            .iter()
            .map(|chord| chord.label())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

fn footer_icon(ui: &mut egui::Ui, icon: Icon, selected: bool, tooltip: &str) -> egui::Response {
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
    crate::icons::paint(painter, rect, icon, color);
    response.on_hover_text(tooltip)
}

fn footer_action(ui: &mut egui::Ui, action: &str, tooltip: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(action).color(theme::TEXT_MUTED)).frame(false))
        .on_hover_text(tooltip)
        .clicked()
}

pub(crate) fn cpu_component(ui: &mut egui::Ui, cpu: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(60.0, 34.0), egui::Sense::click());
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
    response.on_hover_text("DSP load — click for analysis")
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
    crate::icons::paint(painter, rect, Icon::Record, Color32::from_rgb(231, 91, 82));
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

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn handle_shortcuts(
        ui: &mut NotwigUi,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        ui.handle_shortcuts(ctx, doc, runtime);
    }

    pub fn begin_shortcut_capture(ui: &mut NotwigUi, command: crate::commands::ShortcutCommand) {
        ui.shortcut_editor.capture = Some(command);
    }

    pub fn capture_shortcut(ui: &mut NotwigUi, ctx: &egui::Context) {
        ui.capture_shortcut(ctx);
    }
}
