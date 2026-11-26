use notwig_core::edit::SelectionMode;
use notwig_core::{ProjectDoc, TrackKind};

use crate::Clipboard;
use crate::commands::ShortcutCommand;
use crate::intent::UiIntent;
use crate::runtime::RuntimeView;
use crate::state::{EditFocus, OpenedEditor};

use super::NotwigUi;

impl NotwigUi {
    fn loop_range_for_selected_clips(
        doc: &ProjectDoc,
        clips: &[notwig_core::ClipId],
    ) -> Option<notwig_core::RangeMus> {
        let mut bounds: Option<notwig_core::RangeMus> = None;
        for id in clips {
            let Some(clip) = doc.arranger.clips.get(id) else {
                continue;
            };
            bounds = Some(match bounds {
                Some(current) => notwig_core::RangeMus {
                    start: current.start.min(clip.range.start),
                    end: current.end.max(clip.range.end),
                },
                None => clip.range,
            });
        }
        bounds.map(|bounds| {
            let start = bounds.start.max(notwig_core::TimeMus::zero());
            let end = bounds.end.max(start);
            notwig_core::RangeMus { start, end }
        })
    }
    pub(super) fn handle_shortcuts(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
    ) {
        self.handle_shortcuts_inner(ctx, doc, runtime, None);
    }

    pub(super) fn handle_shortcuts_inner(
        &mut self,
        ctx: &egui::Context,
        doc: &ProjectDoc,
        runtime: &RuntimeView,
        forced: Option<ShortcutCommand>,
    ) {
        if forced.is_none() && self.shortcut_editor.capture.is_some() {
            return;
        }
        if forced.is_none() && self.extension_panel_keyboard {
            return;
        }
        if forced.is_none() && ctx.wants_keyboard_input() {
            return;
        }
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::CommandPalette,
        ) {
            self.open_command_palette();
            return;
        }
        if self.command_palette.is_some() {
            return;
        }
        let clip_editor_focused = self.edit_focus == EditFocus::ClipEditor
            && self.currently_opened == Some(OpenedEditor::ClipEditor)
            && self.clip_editor.is_some();
        let midi_editor_focused = self.edit_focus == EditFocus::MidiEditor
            && self.currently_opened == Some(OpenedEditor::MidiEditor)
            && self.midi_editor.is_some();
        let editor_focused = clip_editor_focused || midi_editor_focused;
        let time = self
            .state
            .selection
            .time
            .as_ref()
            .map(|time| (time.range, time.tracks.iter().copied().collect::<Vec<_>>()));
        let clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
        let devices: Vec<_> = self.state.selection.devices.iter().copied().collect();
        let target = self.state.selection.to_edit_selection().mode();
        let copy_time = if target == Some(SelectionMode::Time) {
            time.clone()
        } else {
            None
        };
        let (mut copy, mut cut, mut paste) = self.shortcuts.copy_cut_paste_pressed(ctx);
        copy |= forced == Some(ShortcutCommand::Copy);
        cut |= forced == Some(ShortcutCommand::Cut);
        paste |= forced == Some(ShortcutCommand::Paste);
        if copy && !editor_focused {
            if devices.is_empty() {
                ctx.copy_text("notwig-clipboard".into());
                self.emit(UiIntent::CopySelection {
                    clips: clips.clone(),
                    time: copy_time.clone(),
                });
            } else {
                self.emit(UiIntent::CopyDevices(devices.clone()));
            }
        }
        if cut && !editor_focused {
            if devices.is_empty() {
                ctx.copy_text("notwig-clipboard".into());
                self.emit(UiIntent::CutSelection {
                    clips: clips.clone(),
                    time: copy_time.clone(),
                });
            } else {
                self.emit(UiIntent::CutDevices(devices.clone()));
            }
        }
        if paste && !editor_focused {
            if devices.is_empty() {
                match &runtime.clipboard {
                    Clipboard::Curve(_) => {}
                    Clipboard::Devices => {
                        let track_id = self.selected_track(doc).or_else(|| {
                            self.last_pointer
                                .and_then(|pointer| self.track_at(pointer.y))
                        });
                        if let Some(track_id) = track_id
                            && let Some(track) = doc.tracks.get(&track_id)
                        {
                            let chain_id = track.main_chain;
                            let index = doc
                                .chains
                                .get(&chain_id)
                                .map_or(0, |chain| chain.devices.len());
                            self.emit(UiIntent::PasteDevices {
                                track_id,
                                chain_id,
                                index,
                            });
                        }
                    }
                    Clipboard::Arranger(_) | Clipboard::AudioClip(_) | Clipboard::Empty => {
                        let at = time
                            .as_ref()
                            .map_or(runtime.playhead, |(range, _)| range.start);
                        self.emit(UiIntent::PasteClips { at });
                    }
                }
            } else {
                let device_id = self
                    .state
                    .selection
                    .devices
                    .primary()
                    .or_else(|| devices.first().copied());
                if let Some(device_id) = device_id
                    && let Some((chain_id, index)) = device_chain_location(doc, device_id)
                    && let Some(track_id) = self
                        .state
                        .selection
                        .tracks
                        .primary()
                        .or_else(|| self.selected_track(doc))
                {
                    self.emit(UiIntent::PasteDevices {
                        track_id,
                        chain_id,
                        index: index + 1,
                    });
                }
            }
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Record) {
            self.emit(UiIntent::ToggleRecord);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Stop) {
            self.emit(UiIntent::Stop);
        } else if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Play) {
            if doc.transport.playing {
                self.emit(UiIntent::Stop);
            } else {
                self.emit(UiIntent::Play);
            }
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::SaveAs) {
            self.emit(UiIntent::SaveAs);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Save) {
            self.emit(UiIntent::Save);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::NewProject) {
            self.emit(UiIntent::NewProject);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::OpenProject) {
            self.emit(UiIntent::OpenProject);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::OpenSettings)
            || is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::AudioSettings)
        {
            self.open_settings(runtime);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::LoopSelection) {
            if let Some((range, _)) = time.clone() {
                self.emit(UiIntent::SetLoopRange(range));
            } else if let Some(range) = Self::loop_range_for_selected_clips(doc, &clips) {
                self.emit(UiIntent::SetLoopRange(range));
            } else {
                self.emit(UiIntent::ToggleLoop);
            }
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::ToggleLoop) {
            self.emit(UiIntent::ToggleLoop);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Undo) {
            self.emit(UiIntent::Undo);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Redo) {
            self.emit(UiIntent::Redo);
        }
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::NewInstrumentTrack,
        ) {
            self.emit(UiIntent::CreateTrack(TrackKind::Instrument));
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::NewAudioTrack) {
            self.emit(UiIntent::CreateTrack(TrackKind::Audio));
        }
        if !editor_focused && is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::SelectAll)
        {
            let clips: Vec<_> = doc
                .arranger
                .clips
                .values()
                .map(|clip| (clip.id, clip.track_id))
                .collect();
            self.state.selection.select_clips(&clips);
        }
        if !editor_focused && is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::JoinClips)
        {
            let mut clips: Vec<_> = self.state.selection.clips.iter().copied().collect();
            if clips.len() < 2
                && let Some((range, tracks)) = &time
                && !range.is_empty()
            {
                clips = doc
                    .arranger
                    .clips
                    .values()
                    .filter(|clip| tracks.contains(&clip.track_id))
                    .filter(|clip| clip.range.intersects(*range))
                    .map(|clip| clip.id)
                    .collect();
                clips.sort_unstable();
            }
            if clips.len() >= 2 {
                self.emit(UiIntent::JoinClips { clips });
            }
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::GroupTracks) {
            let tracks: Vec<_> = self.state.selection.tracks.iter().copied().collect();
            if !tracks.is_empty() {
                self.emit(UiIntent::GroupTracks { tracks });
            }
        }
        if !editor_focused
            && is_triggered(
                &self.shortcuts,
                ctx,
                forced,
                ShortcutCommand::ToggleTrackActive,
            )
        {
            if devices.is_empty() {
                let tracks: Vec<_> = self
                    .state
                    .selection
                    .tracks
                    .iter()
                    .copied()
                    .filter(|track| {
                        doc.tracks
                            .get(track)
                            .is_some_and(|track| track.kind != TrackKind::Master)
                    })
                    .collect();
                let active = tracks
                    .iter()
                    .any(|track| doc.tracks.get(track).is_some_and(|track| track.active));
                for track in tracks {
                    self.emit(UiIntent::SetTrackActive {
                        track_id: track,
                        active: !active,
                    });
                }
            } else {
                let active = devices
                    .iter()
                    .any(|id| doc.devices.get(id).is_some_and(|device| device.active));
                self.emit(UiIntent::SetDevicesActive {
                    device_ids: devices.clone(),
                    active: !active,
                });
            }
        }
        if !editor_focused && is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Duplicate)
        {
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
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::FitArrangement,
        ) {
            self.zoom_to_fit(doc);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::ImportAudio) {
            self.emit(UiIntent::ImportAudio);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::RenderMixdown) {
            self.open_render(doc);
        }
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::ToggleInfoPanel,
        ) {
            self.show_info = !self.show_info;
        }
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::ToggleDeviceEditor,
        ) {
            self.toggle_device_editor();
        }
        if is_triggered(
            &self.shortcuts,
            ctx,
            forced,
            ShortcutCommand::ToggleClipEditor,
        ) {
            self.toggle_clip_editor(doc);
        }
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Cancel)
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
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Delete) && !editor_focused {
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
        if is_triggered(&self.shortcuts, ctx, forced, ShortcutCommand::Quit) {
            self.emit(UiIntent::Quit);
        }
        if forced.is_none() {
            let extension_commands = self.commands.extension_commands();
            if extension_commands
                .iter()
                .any(|command| self.shortcuts.was_pressed_id(ctx, &command.id))
            {
                let context = self.command_context(doc, runtime);
                for command in extension_commands {
                    if self.shortcuts.was_pressed_id(ctx, &command.id) && command.enabled(&context)
                    {
                        self.emit(UiIntent::RunExtensionCommand {
                            command_id: command.id.clone(),
                            context: context.clone(),
                        });
                    }
                }
            }
        }
    }
}

fn device_chain_location(
    doc: &ProjectDoc,
    device_id: notwig_core::DeviceId,
) -> Option<(notwig_core::DeviceChainId, usize)> {
    doc.chains.iter().find_map(|(chain_id, chain)| {
        chain
            .devices
            .iter()
            .position(|id| *id == device_id)
            .map(|index| (*chain_id, index))
    })
}

fn is_triggered(
    shortcuts: &crate::commands::Shortcuts,
    ctx: &egui::Context,
    forced: Option<ShortcutCommand>,
    command: ShortcutCommand,
) -> bool {
    forced == Some(command) || shortcuts.was_pressed(ctx, command)
}
