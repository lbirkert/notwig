use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::SystemTime;

use egui::Rect;
use notwig_core::dsp::DeviceRegistry;
use notwig_core::{ProjectDoc, TrackId};

use crate::arranger::{BarSelectionDrag, ClipDrag, GroupDrag, TimeDrag, TrackDrop};
use crate::commands::{CommandRegistry, Shortcuts};
use crate::drop_target::DragPreview;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::{EditFocus, OpenedEditor, UiState};
use crate::theme;
use crate::views::audio_editor::ClipEditorState;
use crate::views::device_editor::{DeviceBrowserState, DeviceDrag, DeviceInsertion, EqDrag};
use crate::views::midi_editor::MidiEditorState;

mod app_bar;
mod command_palette;
mod drag_drop;
mod help;
mod layout;
mod overlays;
mod shortcuts;
mod splash;
mod status_bar;
mod top_bar;

pub(crate) use layout::track_depth;

pub struct NotwigUi {
    pub state: UiState,
    pub intents: Vec<UiIntent>,
    pub registry: DeviceRegistry,
    pub(crate) device_panel_host: Option<Box<dyn notwig_ext_api::DevicePanelHost>>,
    pub(crate) audio_sub_editor_host: Option<Box<dyn notwig_ext_api::AudioSubEditorHost>>,
    pub(crate) extension_panel_textures: BTreeMap<String, BTreeMap<u64, egui::TextureHandle>>,
    pub(crate) last_extension_frames: BTreeMap<String, notwig_ext_api::PanelFrame>,
    pub(crate) extension_panel_keyboard: bool,
    pub(crate) extension_audio_sub_editors: Vec<notwig_ext_api::AudioSubEditorDescriptor>,
    pub(crate) extension_index_input: String,
    pub(crate) extension_index_requested: bool,
    pub(crate) extension_filter: crate::settings::ExtensionFilter,
    pub(crate) show_extension_index_manager: bool,
    pub(crate) pending_extension_replace: Option<String>,
    pub(crate) extension_images:
        BTreeMap<PathBuf, (Option<SystemTime>, Option<egui::TextureHandle>)>,
    pub(crate) extension_icon_images: BTreeMap<String, (u64, Option<egui::TextureHandle>)>,
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
    pub collapsed_tracks: BTreeSet<TrackId>,
    pub timeline_width: f32,
    pub timeline_left: f32,
    pub arranger_header_width: f32,
    pub arranger_rect: Rect,
    pub last_pointer: Option<egui::Pos2>,
    pub loop_drag_start: Option<notwig_core::TimeMus>,
    pub eq_band: usize,
    pub eq_range: f32,
    pub eq_drag: Option<EqDrag>,
    pub drag_preview: Option<DragPreview>,
    pub expanded_chains: BTreeSet<notwig_core::DeviceChainId>,
    pub clip_editor: Option<ClipEditorState>,
    pub clip_editor_content_rect: Rect,
    pub midi_editor: Option<MidiEditorState>,
    pub midi_editor_content_rect: Rect,
    pub edit_focus: EditFocus,
    pub plugin_search: BTreeMap<notwig_core::DeviceId, String>,
    pub last_touched_param: BTreeMap<notwig_core::DeviceId, notwig_core::ParamId>,
    pub collapsed_devices: BTreeSet<notwig_core::DeviceId>,
    pub device_browser: Option<DeviceBrowserState>,
    pub(crate) device_drag: Option<DeviceDrag>,
    pub(crate) device_insertions: Vec<DeviceInsertion>,
    pub show_info: bool,
    pub currently_opened: Option<OpenedEditor>,
    pub settings_open: bool,
    pub settings_category: crate::settings::SettingsCategory,
    pub audio_draft: crate::runtime::AudioSettingsValue,
    pub(crate) shortcuts: Shortcuts,
    pub(crate) commands: CommandRegistry,
    pub(crate) command_palette: Option<command_palette::CommandPaletteState>,
    pub(crate) pending_builtin_command: Option<crate::commands::ShortcutCommand>,
    pub(crate) shortcut_editor: crate::commands::ShortcutEditorState,
    pub dsp_open: bool,
    pub render_open: bool,
    pub render_start: notwig_core::TimeMus,
    pub render_end: notwig_core::TimeMus,
    pub render_options: crate::intent::ExportOptions,
    pub render_progress: Option<f32>,
    pub(crate) project_picker_epoch: u64,
    pub(crate) project_picker_name: String,
    pub(crate) project_picker_confirm_overwrite: bool,
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
            registry: notwig_core::dsp::stock_registry(),
            device_panel_host: None,
            audio_sub_editor_host: None,
            extension_panel_textures: BTreeMap::new(),
            last_extension_frames: BTreeMap::new(),
            extension_panel_keyboard: false,
            extension_audio_sub_editors: Vec::new(),
            extension_index_input: String::new(),
            extension_index_requested: false,
            extension_filter: crate::settings::ExtensionFilter::default(),
            show_extension_index_manager: false,
            pending_extension_replace: None,
            extension_images: BTreeMap::new(),
            extension_icon_images: BTreeMap::new(),
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
            collapsed_tracks: BTreeSet::new(),
            timeline_width: 800.0,
            timeline_left: 230.0,
            arranger_header_width: 230.0,
            arranger_rect: Rect::NOTHING,
            last_pointer: None,
            loop_drag_start: None,
            eq_band: 1,
            eq_range: 10.0,
            eq_drag: None,
            drag_preview: None,
            expanded_chains: BTreeSet::new(),
            clip_editor: None,
            clip_editor_content_rect: Rect::NOTHING,
            midi_editor: None,
            midi_editor_content_rect: Rect::NOTHING,
            edit_focus: EditFocus::Timeline,
            plugin_search: BTreeMap::new(),
            last_touched_param: BTreeMap::new(),
            collapsed_devices: BTreeSet::new(),
            device_browser: None,
            device_drag: None,
            device_insertions: Vec::new(),
            show_info: true,
            currently_opened: Some(OpenedEditor::DeviceEditor),
            settings_open: false,
            settings_category: crate::settings::SettingsCategory::Audio,
            audio_draft: crate::runtime::AudioSettingsValue::default(),
            shortcuts: Shortcuts::default(),
            commands: CommandRegistry::new(),
            command_palette: None,
            pending_builtin_command: None,
            shortcut_editor: crate::commands::ShortcutEditorState::default(),
            dsp_open: false,
            render_open: false,
            render_start: notwig_core::TimeMus::zero(),
            render_end: notwig_core::TimeMus::zero(),
            render_options: crate::intent::ExportOptions::default(),
            render_progress: None,
            project_picker_epoch: 0,
            project_picker_name: String::new(),
            project_picker_confirm_overwrite: false,
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
    pub fn set_device_panel_host(
        &mut self,
        host: Option<Box<dyn notwig_ext_api::DevicePanelHost>>,
    ) {
        self.device_panel_host = host;
        self.last_extension_frames.clear();
        self.extension_panel_textures.clear();
    }

    pub fn set_audio_sub_editor_host(
        &mut self,
        host: Option<Box<dyn notwig_ext_api::AudioSubEditorHost>>,
    ) {
        self.audio_sub_editor_host = host;
        self.extension_panel_textures.clear();
    }

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
        self.update_drag_preview(ctx, doc, runtime);
        self.track_rows.clear();
        if let Some(command) = self.pending_builtin_command.take() {
            self.handle_shortcuts_inner(ctx, doc, runtime, Some(command));
        } else {
            self.handle_shortcuts(ctx, doc, runtime);
        }
        self.extension_panel_keyboard = false;
        Self::app_bar(ctx, doc, runtime);
        self.top_bar(ctx, doc, runtime);
        self.status_bar(ctx, doc, runtime);
        self.info_panel(ctx, doc, runtime);
        if let Some(editor) = self.currently_opened
            && let Some(panel) = crate::views::panel_registry::core_bottom_panel(editor)
        {
            (panel.render)(self, ctx, doc, runtime);
        }
        self.device_browser_window(ctx, doc, runtime);
        self.command_palette_window(ctx, doc, runtime);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| self.arranger_view(ui, doc, runtime));
        self.draw_device_drag_preview(ctx, doc);
        self.finish_device_drag(ctx, doc);
        if self.settings_open {
            self.settings_window(ctx, runtime);
        }
        if self.dsp_open {
            self.dsp_window(ctx, runtime);
        }
        if self.render_open {
            self.render_window(ctx, doc);
        }
        self.project_picker_window(ctx, runtime);
        if self.show_notifications {
            self.notifications_window(ctx);
        }
        if self.show_help {
            self.help_window(ctx);
        }
        if runtime.engine_crashed {
            self.engine_crash_overlay(ctx);
        }
        if runtime.audio_device_lost {
            self.audio_device_lost_overlay(ctx, runtime);
        }
        if let Some(progress) = self.render_progress {
            self.render_progress_overlay(ctx, progress);
        }
        self.notification_overlay(ctx);
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
        let extension_commands = self.commands.extension_commands();
        self.shortcuts.add_defaults(&extension_commands);
    }

    /// Replaces the extension command catalog and adds missing default
    /// bindings. Built-in commands remain authoritative.
    pub fn set_extension_commands(&mut self, commands: Vec<notwig_ext_api::CommandDescriptor>) {
        self.commands.replace_extension_commands(commands.clone());
        self.shortcuts.add_defaults(&commands);
    }

    /// Replaces the audio-editor sub-editors supplied by loaded extensions.
    ///
    /// An active extension sub-editor that disappears falls back to Audio
    /// Events instead of leaving the editor on a missing surface.
    pub fn set_extension_audio_sub_editors(
        &mut self,
        editors: Vec<notwig_ext_api::AudioSubEditorDescriptor>,
    ) {
        self.extension_audio_sub_editors = editors;
        let still_present = self.clip_editor.as_ref().is_some_and(|state| {
            state
                .sub_editor
                .extension_id()
                .is_none_or(|id| self.extension_audio_sub_editors.iter().any(|e| e.id == id))
        });
        if !still_present {
            if let Some(state) = &mut self.clip_editor {
                state.switch_sub_editor(crate::views::audio_editor::AudioSubEditor::AudioEvents);
            }
        }
    }

    /// Returns the resolved command catalog.
    #[must_use]
    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
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
        self.midi_editor = None;
        self.edit_focus = EditFocus::ClipEditor;
    }

    pub fn open_midi_editor(&mut self, clip_id: notwig_core::ClipId) {
        self.currently_opened = Some(OpenedEditor::MidiEditor);
        self.midi_editor = Some(MidiEditorState::new(clip_id));
        self.clip_editor = None;
        self.edit_focus = EditFocus::MidiEditor;
    }

    pub fn open_editor_for_clip(&mut self, doc: &ProjectDoc, clip_id: notwig_core::ClipId) {
        let Some(clip) = doc.arranger.clips.get(&clip_id) else {
            return;
        };
        self.state
            .selection
            .select_clip(clip_id, clip.track_id, false);
        match &clip.payload {
            notwig_core::ClipPayload::Midi(_) => self.open_midi_editor(clip_id),
            notwig_core::ClipPayload::Audio(_) => self.open_clip_editor(clip_id),
        }
    }

    pub fn close_clip_editor(&mut self) {
        if matches!(
            self.currently_opened,
            Some(OpenedEditor::ClipEditor | OpenedEditor::MidiEditor)
        ) {
            self.currently_opened = None;
        }
        self.clip_editor = None;
        self.midi_editor = None;
        self.edit_focus = EditFocus::Timeline;
    }

    pub fn close_midi_editor(&mut self) {
        self.close_clip_editor();
    }

    pub fn open_device_editor(&mut self) {
        self.currently_opened = Some(OpenedEditor::DeviceEditor);
        self.clip_editor = None;
        self.midi_editor = None;
        self.edit_focus = EditFocus::Timeline;
    }

    pub fn close_editor(&mut self) {
        self.currently_opened = None;
        self.clip_editor = None;
        self.midi_editor = None;
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
            Some(OpenedEditor::ClipEditor | OpenedEditor::MidiEditor)
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
            self.open_midi_editor(clip_id);
        } else if let Some(clip_id) = self.selected_audio_clip(doc) {
            self.open_clip_editor(clip_id);
        } else {
            self.currently_opened = Some(OpenedEditor::ClipEditor);
            self.clip_editor = None;
            self.midi_editor = None;
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
        if self.currently_opened != Some(OpenedEditor::DeviceEditor)
            || self
                .device_drag
                .is_some_and(|drag| !doc.devices.contains_key(&drag.device_id))
        {
            self.device_drag = None;
        }
        if self.currently_opened != Some(OpenedEditor::DeviceEditor) {
            self.device_insertions.clear();
        }
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
            Some(OpenedEditor::MidiEditor) => {
                let selected = self.selected_midi_clip(doc);
                match (
                    selected,
                    self.midi_editor.as_ref().map(|state| state.clip_id),
                ) {
                    (Some(id), Some(current)) if current == id => {}
                    (Some(id), _) => self.midi_editor = Some(MidiEditorState::new(id)),
                    (None, _) => self.midi_editor = None,
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

    pub fn set_extension_commands(
        ui: &mut NotwigUi,
        commands: Vec<notwig_ext_api::CommandDescriptor>,
    ) {
        ui.set_extension_commands(commands);
    }

    pub fn set_extension_audio_sub_editors(
        ui: &mut NotwigUi,
        editors: Vec<notwig_ext_api::AudioSubEditorDescriptor>,
    ) {
        ui.set_extension_audio_sub_editors(editors);
    }

    pub fn set_audio_sub_editor_host(
        ui: &mut NotwigUi,
        host: Option<Box<dyn notwig_ext_api::AudioSubEditorHost>>,
    ) {
        ui.set_audio_sub_editor_host(host);
    }

    pub fn open_command_palette(ui: &mut NotwigUi) {
        ui.open_command_palette();
    }

    #[must_use]
    pub fn command_palette_open(ui: &NotwigUi) -> bool {
        ui.command_palette.is_some()
    }

    pub fn command_context(
        ui: &NotwigUi,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) -> notwig_ext_api::CommandContext {
        ui.command_context(doc, runtime)
    }

    pub fn begin_shortcut_capture(ui: &mut NotwigUi, command: crate::commands::ShortcutCommand) {
        ui.shortcut_editor.capture = Some(command.id().to_owned());
    }

    pub fn capture_shortcut(ui: &mut NotwigUi, ctx: &egui::Context) {
        ui.capture_shortcut(ctx);
    }

    pub fn row_rect(ui: &NotwigUi, track: TrackId) -> Option<egui::Rect> {
        ui.row_rect(track)
    }
}
