use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use mouse_position::mouse_position::Mouse;
use notwig_core::protocol::CommandPayload;
use notwig_core::{DeviceId, DeviceKind, ProjectDoc, Rational, TimeMus, TrackKind};
use notwig_plugins::NativePluginWindow;
use notwig_ui::{DropAudioTarget, NotificationKind, NotwigUi, RuntimeView, UiIntent};

use crate::audio::AudioService;
use crate::workspace::Workspace;

pub fn run(initial: Option<PathBuf>) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Notwig")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Notwig",
        options,
        Box::new(move |_cc| Ok(Box::new(NotwigApp::new(initial)))),
    )
    .map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Copy)]
enum DialogAction {
    OpenProject,
    SaveAs,
    Render { start: TimeMus, end: TimeMus },
    ImportAudio,
    Relink(notwig_core::AssetId),
}

struct NotwigApp {
    workspace: Workspace,
    ui: NotwigUi,
    audio: Option<AudioService>,
    audio_settings: crate::audio::AudioSettings,
    audio_options: Vec<notwig_ui::AudioDriverView>,
    fallback_playhead: f64,
    last_tick: Instant,
    dialog_rx: Option<mpsc::Receiver<(DialogAction, Option<PathBuf>)>>,
    pending_drops: Vec<(PathBuf, DropAudioTarget, TimeMus)>,
    plugin_editors: Vec<(DeviceId, NativePluginWindow)>,
    recovery_error: Option<String>,
}

impl NotwigApp {
    fn new(initial: Option<PathBuf>) -> Self {
        let opened = match initial.as_deref() {
            Some(path) => Workspace::open(path),
            None => Workspace::new_empty(),
        };
        let mut workspace = match opened {
            Ok(workspace) => workspace,
            Err(err) => {
                let mut workspace = Workspace::new_empty().expect("create fallback workspace");
                workspace.status = Some(format!("Could not open project: {err}"));
                workspace
            }
        };
        let fallback_playhead = workspace.doc.transport.playhead.as_f64();
        let audio_options = AudioService::available_options();
        let mut audio_settings =
            crate::audio_config::validated(crate::audio_config::load(), &audio_options);
        audio_settings.sample_rate = Some(workspace.doc.meta.sample_rate.value());
        if audio_settings.block_size == 0 {
            audio_settings.block_size = workspace.audio_block_size;
        }
        if workspace.audio_block_size != audio_settings.block_size {
            workspace.audio_block_size = audio_settings.block_size;
            workspace.request_engine_rebuild();
        }
        let mut app = Self {
            workspace,
            ui: NotwigUi::default(),
            audio: None,
            audio_settings,
            audio_options,
            fallback_playhead,
            last_tick: Instant::now(),
            dialog_rx: None,
            pending_drops: Vec::new(),
            plugin_editors: Vec::new(),
            recovery_error: None,
        };
        app.ui.set_shortcuts(crate::shortcuts::load());
        app.restart_audio();
        app
    }

    fn restart_audio(&mut self) {
        self.audio = None;
        let rate = self.workspace.doc.meta.sample_rate.value();
        match AudioService::start_with(
            self.workspace.engine_control_handle(),
            rate,
            &self.audio_settings,
        ) {
            Ok(audio) => {
                self.audio = Some(audio);
                self.workspace.engine_audio_restarted();
            }
            Err(err) => self.workspace.status = Some(err),
        }
    }

    fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(AudioService::snapshot)
            .unwrap_or_default()
    }

    fn runtime_view(&self, snapshot: &crate::audio::AudioSnapshot) -> RuntimeView {
        let map = self.workspace.doc.tempo_map();
        let fallback_time = f64_to_time(self.fallback_playhead);
        let playhead = if snapshot.running {
            map.samples_to_mus(snapshot.playhead_samples)
        } else {
            fallback_time
        };
        let meters = snapshot.meters_for_display(self.workspace.doc.transport.playing);
        let status = snapshot
            .error
            .clone()
            .or_else(|| self.workspace.status.clone())
            .or_else(|| {
                snapshot.running.then(|| {
                    format!(
                        "Audio: {} · {} Hz · {} ch",
                        snapshot.device_name, snapshot.sample_rate, snapshot.channels
                    )
                })
            });
        let audio_settings = crate::audio::settings_view(
            self.audio_options.clone(),
            &self.audio_settings,
            snapshot.error.clone(),
            !self.workspace.assets.is_empty(),
        );
        RuntimeView {
            playing: self.workspace.doc.transport.playing,
            playhead,
            play_start: self
                .workspace
                .play_start
                .unwrap_or(self.workspace.doc.transport.playhead),
            waveforms: self.workspace.peaks.clone(),
            meters,
            xruns: snapshot.xruns,
            cpu_load: snapshot.cpu_load,
            status,
            plugins: self.workspace.plugins.clone(),
            bundle_path: self.workspace.bundle.clone(),
            dirty: self.workspace.dirty,
            saving: false,
            audio_settings,
            dsp: snapshot.dsp.clone(),
            clipboard: self.workspace.clipboard().clone(),
        }
    }

    fn advance_fallback(&mut self, playing: bool) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        if playing {
            let bpm = self.workspace.doc.meta.tempo_bpm;
            let beats_per_second = bpm / 60.0;
            self.fallback_playhead += elapsed * beats_per_second;
            if self.workspace.doc.transport.loop_enabled {
                if let Some(range) = self.workspace.doc.transport.loop_range {
                    let start = range.start.as_f64();
                    let end = range.end.as_f64();
                    if end > start {
                        while self.fallback_playhead >= end {
                            self.fallback_playhead -= end - start;
                        }
                    }
                }
            }
        } else {
            self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
        }
    }

    fn current_playhead(&self, snapshot: &crate::audio::AudioSnapshot) -> TimeMus {
        if snapshot.running && self.workspace.doc.transport.playing {
            self.workspace
                .doc
                .tempo_map()
                .samples_to_mus(snapshot.playhead_samples)
        } else if !snapshot.running && self.workspace.doc.transport.playing {
            f64_to_time(self.fallback_playhead)
        } else {
            self.workspace.doc.transport.playhead
        }
    }

    fn sync_playhead(&mut self) {
        let snapshot = self.audio_snapshot();
        let playhead = self.current_playhead(&snapshot);
        let _ = self.workspace.set_playhead(playhead);
        self.fallback_playhead = playhead.as_f64();
    }

    fn notify_error(&mut self, message: String) {
        self.workspace.error = Some(message.clone());
        self.workspace.status = Some(message.clone());
        self.ui.state.notify(NotificationKind::Error, message);
    }

    fn report<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.notify_error(err);
                None
            }
        }
    }

    fn notify_info(&mut self, message: String) {
        self.ui
            .state
            .notify(NotificationKind::Info, message.clone());
        self.workspace.status = Some(message);
    }

    fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        if !self.workspace.recovery_offered {
            return;
        }
        let mut recover = false;
        let mut discard = false;
        egui::Modal::new(egui::Id::new("recovery-modal")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading("Recover unsaved changes?");
            ui.label("A previous session did not close cleanly. Autosave snapshots are available.");
            ui.label(format!("Project: {}", self.workspace.doc.meta.name));
            if let Some(bundle) = &self.workspace.bundle {
                ui.label(format!("Bundle: {}", bundle.display()));
            }
            if let Some(error) = &self.recovery_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Recover latest autosave").clicked() {
                    recover = true;
                }
                if ui.button("Discard").clicked() {
                    discard = true;
                }
            });
        });
        if recover {
            match self.workspace.recover_latest_autosave() {
                Ok(()) => {
                    self.recovery_error = None;
                    self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                    self.ui.state.selection.clear();
                    self.restart_audio();
                    self.notify_info("Recovered latest autosave.".into());
                }
                Err(err) => {
                    self.recovery_error = Some(format!("Could not recover autosave: {err}"));
                }
            }
        } else if discard {
            match self.workspace.discard_recovery() {
                Ok(()) => {
                    self.recovery_error = None;
                    self.workspace.begin_session();
                    self.notify_info("Discarded recovery autosaves.".into());
                }
                Err(err) => {
                    self.recovery_error = Some(format!("Could not discard autosaves: {err}"));
                }
            }
        }
    }

    fn open_plugin_window(&mut self, device_id: DeviceId) {
        if let Some((_, window)) = self.plugin_editors.iter().find(|(id, _)| *id == device_id) {
            window.focus();
            return;
        }
        let Some(descriptor) = self.workspace.plugin_descriptor(device_id) else {
            self.notify_error("Plugin is not installed or was not found.".into());
            return;
        };
        match NativePluginWindow::open(&descriptor) {
            Ok(mut window) => {
                if let Some(device) = self.workspace.doc.devices.get(&device_id) {
                    for (param_id, value) in &device.param_values {
                        if let Some(value) = value.as_f64() {
                            window.set_parameter((*param_id).into_uuid().as_u128() as u32, value);
                        }
                    }
                }
                self.plugin_editors.push((device_id, window));
            }
            Err(err) => {
                self.notify_error(format!("Could not open plugin window: {err}"));
            }
        }
    }

    fn handle_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::Play => {
                let result = self.workspace.play();
                let _ = self.report(result);
            }
            UiIntent::Pause => {
                self.sync_playhead();
                let result = self.workspace.pause();
                let _ = self.report(result);
            }
            UiIntent::Stop => {
                let result = self.workspace.stop();
                let _ = self.report(result);
                self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
            }
            UiIntent::ToggleLoop => {
                let result = self.workspace.toggle_loop();
                let _ = self.report(result);
            }
            UiIntent::SetTempo(tempo_bpm) => {
                let result = self.workspace.apply(CommandPayload::SetTempo { tempo_bpm });
                let _ = self.report(result);
            }
            UiIntent::SetTimeSignature {
                numerator,
                denominator,
            } => {
                let result = self.workspace.apply(CommandPayload::SetTimeSignature {
                    numerator,
                    denominator,
                });
                let _ = self.report(result);
            }
            UiIntent::SetPlayhead(time) => {
                self.fallback_playhead = time.as_f64();
                let result = self.workspace.set_playhead(time);
                let _ = self.report(result);
            }
            UiIntent::SetPlayStart(time) => {
                if self.workspace.doc.transport.playing {
                    self.workspace.play_start = Some(time);
                } else {
                    self.workspace.play_start = Some(time);
                    self.fallback_playhead = time.as_f64();
                    let result = self.workspace.set_playhead(time);
                    let _ = self.report(result);
                }
            }
            UiIntent::SetLoopRange(range) => {
                let result = self.workspace.apply(CommandPayload::SetTransport {
                    playing: None,
                    playhead: None,
                    loop_enabled: Some(true),
                    loop_range: Some(Some(range)),
                });
                let _ = self.report(result);
            }
            UiIntent::Quit
            | UiIntent::SelectTrack(_)
            | UiIntent::SelectClip(_)
            | UiIntent::SelectDevice(_) => {}
            UiIntent::CreateTrack(kind) => {
                let result = self.workspace.create_track(kind);
                let _ = self.report(result);
            }
            UiIntent::DeleteTrack(track_id) => {
                let result = self
                    .workspace
                    .apply(CommandPayload::DeleteTrack { id: track_id });
                let _ = self.report(result);
            }
            UiIntent::DuplicateTrack(track_id) => match self.workspace.duplicate_track(track_id) {
                Ok(track) => {
                    self.ui.state.selection.select_track(track, false);
                    self.notify_info("Duplicated track.".into());
                }
                Err(err) => self.notify_error(err),
            },
            UiIntent::DeleteClip(clip_id) => {
                let result = self.workspace.delete_selection(&[clip_id], None);
                let _ = self.report(result);
            }
            UiIntent::CreateMidiClip { track_id, range } => {
                let result = self.workspace.create_midi_clip(track_id, range);
                let _ = self.report(result);
            }
            UiIntent::CreateAudioClip { track_id, range } => {
                let result = self.workspace.create_audio_clip(track_id, range);
                let _ = self.report(result);
            }
            UiIntent::EditClipRange { clip_id, range } => {
                let result = self.workspace.trim_clip(clip_id, range);
                let _ = self.report(result);
            }
            UiIntent::SetClipRange { clip_id, range } => {
                let result = self.workspace.set_clip_range(clip_id, range);
                let _ = self.report(result);
            }
            UiIntent::MoveClip {
                clip_id,
                track_id,
                range,
            } => {
                let result = self.workspace.relocate_clips(&[(clip_id, track_id, range)]);
                let _ = self.report(result);
            }
            UiIntent::MoveClips { clips } => {
                let result = self.workspace.relocate_clips(&clips);
                let _ = self.report(result);
            }
            UiIntent::SetClipMuted { clip_id, muted } => {
                let result = self.workspace.apply(CommandPayload::EditClip {
                    clip_id,
                    range: None,
                    name: None,
                    muted: Some(muted),
                    content_start: None,
                    content_length: None,
                    audio: None,
                });
                let _ = self.report(result);
            }
            UiIntent::DuplicateClip { clip_id } => match self.workspace.duplicate_clip(clip_id) {
                Ok(new_id) => {
                    if let Some(track) = self
                        .workspace
                        .doc
                        .arranger
                        .clips
                        .get(&new_id)
                        .map(|clip| clip.track_id)
                    {
                        self.ui.state.selection.select_clip(new_id, track, false);
                    }
                }
                Err(err) => self.notify_error(err),
            },
            UiIntent::CopySelection { clips, time } => {
                let count = self.workspace.copy_selection(&clips, time);
                if count > 0 {
                    tracing::info!(count, "copied selection");
                    self.notify_info(format!("Copied {count} clip(s)."));
                } else {
                    self.notify_error("Nothing to copy.".into());
                }
            }
            UiIntent::CutSelection { clips, time } => {
                let count = self.workspace.cut_selection(&clips, time.clone());
                if count > 0 {
                    tracing::info!(count, "cut selection");
                    self.notify_info(format!("Cut {count} clip(s)."));
                    if time.is_none() {
                        self.ui.state.selection.clips.clear();
                    }
                } else {
                    self.notify_error("Nothing to cut.".into());
                }
            }
            UiIntent::PasteClips { at } => match self.workspace.paste_clips(at) {
                Ok(info) if !info.is_empty() => {
                    let count = info.objects.len().max(usize::from(info.time.is_some()));
                    tracing::info!(count, "pasted selection");
                    self.notify_info(format!("Pasted {count} object(s)."));
                    self.ui.state.selection.apply_info(&info);
                }
                Ok(_) => self.notify_error("Clipboard is empty.".into()),
                Err(err) => self.notify_error(err),
            },
            UiIntent::SetClipboard(clipboard) => {
                self.workspace.set_clipboard(clipboard);
            }
            UiIntent::ClipboardError(message) => {
                self.notify_error(message);
            }
            UiIntent::DeleteSelection { clips, time } => {
                match self.workspace.delete_selection(&clips, time) {
                    Ok(info) => self.ui.state.selection.apply_info(&info),
                    Err(err) => self.notify_error(err),
                }
            }
            UiIntent::DuplicateSelection { clips, time } => {
                match self.workspace.duplicate_selection(&clips, time) {
                    Ok(info) => {
                        self.ui.state.selection.apply_info(&info);
                        self.notify_info("Duplicated selection.".into());
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            UiIntent::MoveTimeSelection {
                range,
                tracks,
                delta,
            } => match self.workspace.move_time_selection(range, &tracks, delta) {
                Ok(info) => self.ui.state.selection.apply_info(&info),
                Err(err) => self.notify_error(err),
            },
            UiIntent::SetTrackColor { track_id, color } => {
                let result = self.workspace.apply(CommandPayload::SetTrackColor {
                    id: track_id,
                    color,
                });
                let _ = self.report(result);
            }
            UiIntent::RenameTrack { track_id, name } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::RenameTrack { id: track_id, name });
                let _ = self.report(result);
            }
            UiIntent::SetTrackActive { track_id, active } => {
                let result = self.workspace.apply(CommandPayload::SetTrackActive {
                    id: track_id,
                    active,
                });
                let _ = self.report(result);
            }
            UiIntent::SetTrackComment { track_id, comment } => {
                let result = self.workspace.apply(CommandPayload::SetTrackComment {
                    id: track_id,
                    comment,
                });
                let _ = self.report(result);
            }
            UiIntent::MoveTrack { id, parent, index } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::MoveTrack { id, parent, index });
                let _ = self.report(result);
            }
            UiIntent::GroupTracks { tracks } => match self.workspace.group_tracks(&tracks) {
                Ok(group) => {
                    self.ui.state.selection.select_track(group, false);
                    self.notify_info("Grouped tracks.".into());
                }
                Err(err) => self.notify_error(err),
            },
            UiIntent::SetClipSlices { clip_id, slices } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetClipSlices { clip_id, slices });
                let _ = self.report(result);
            }
            UiIntent::CreateMidiNote { clip_id, note } => {
                let result = self.workspace.create_note(clip_id, note);
                let _ = self.report(result);
            }
            UiIntent::EditMidiNotes {
                clip_id,
                notes,
                removed,
            } => {
                let result = self.workspace.edit_notes(clip_id, notes, removed);
                let _ = self.report(result);
            }
            UiIntent::PreviewNote {
                track_id,
                note_id,
                pitch,
                velocity,
            } => self
                .workspace
                .preview_note(track_id, note_id, pitch, velocity),
            UiIntent::SplitClip { clip_id, at } => {
                let result = self.workspace.split_clip(clip_id, at);
                let _ = self.report(result);
            }
            UiIntent::SetTrackMixer {
                track_id,
                volume_db,
                pan,
                mute,
                solo,
            } => {
                let result = self
                    .workspace
                    .set_mixer(track_id, volume_db, pan, mute, solo);
                let _ = self.report(result);
            }
            UiIntent::InsertStockDevice {
                chain_id,
                type_id,
                kind,
                index,
                ..
            } => {
                let result = self.workspace.insert_stock(chain_id, &type_id, kind, index);
                let _ = self.report(result);
            }
            UiIntent::InsertPlugin {
                chain_id,
                type_id,
                kind,
                index,
                ..
            } => match self
                .workspace
                .insert_plugin(chain_id, &type_id, kind, index)
            {
                Ok(device_id) => self.open_plugin_window(device_id),
                Err(err) => self.notify_error(err),
            },
            UiIntent::ImportAudio => {
                if self.workspace.bundle.is_none() {
                    self.notify_error("Save the project before importing audio.".into());
                } else {
                    self.spawn_dialog(DialogAction::ImportAudio, audio_pick_dialog("Import audio"));
                }
            }
            UiIntent::DropAudio { path, target, at } => {
                if self.workspace.bundle.is_none() {
                    self.pending_drops.push((path, target, at));
                    self.handle_save_as();
                } else {
                    self.perform_drop(path, target, at);
                }
            }
            UiIntent::InsertAudioClip { track_id, asset_id } => {
                let result = self.workspace.insert_audio_clip(track_id, asset_id);
                let _ = self.report(result);
            }
            UiIntent::RemoveDevice(device_id) => {
                let result = self
                    .workspace
                    .apply(CommandPayload::RemoveDevice { device_id });
                let _ = self.report(result);
            }
            UiIntent::OpenPluginWindow { device_id } => self.open_plugin_window(device_id),
            UiIntent::SetDeviceBypass { device_id, bypass } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetDeviceBypass { device_id, bypass });
                let _ = self.report(result);
            }
            UiIntent::SetParam {
                device_id,
                param_id,
                value,
            } => {
                let raw_value = value.as_f64();
                let result = self.workspace.apply(CommandPayload::SetParam {
                    device_id,
                    param_id,
                    value,
                    ramp: None,
                });
                let _ = self.report(result);
                if let Some(value) = raw_value
                    && let Some((_, window)) = self
                        .plugin_editors
                        .iter_mut()
                        .find(|(id, _)| *id == device_id)
                {
                    window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
                }
            }
            UiIntent::RelinkAsset(asset_id) => {
                self.spawn_dialog(
                    DialogAction::Relink(asset_id),
                    audio_pick_dialog("Relink audio asset"),
                );
            }
            UiIntent::ApplyAudioSettings {
                driver,
                device,
                sample_rate,
                block_size,
            } => {
                if let Some(rate) = sample_rate
                    && rate != self.workspace.doc.meta.sample_rate.value()
                    && let Err(err) = self.workspace.set_sample_rate(rate)
                {
                    self.notify_error(err);
                    return;
                }
                self.audio_settings = crate::audio::AudioSettings {
                    driver,
                    device,
                    sample_rate,
                    block_size,
                };
                self.workspace.audio_block_size = if block_size == 0 { 512 } else { block_size };
                self.workspace.request_engine_rebuild();
                self.audio_options = AudioService::available_options();
                self.restart_audio();
                if let Err(err) = crate::audio_config::save(&self.audio_settings) {
                    self.notify_error(format!("Audio settings applied but not saved: {err}"));
                } else {
                    self.notify_info("Audio settings applied.".into());
                }
            }
            UiIntent::NewProject | UiIntent::NewProjectAtRate(_) => {
                let rate = match intent {
                    UiIntent::NewProjectAtRate(rate) => rate,
                    _ => 48_000,
                };
                match Workspace::new_empty_at(rate) {
                    Ok(workspace) => {
                        self.workspace.end_session();
                        self.workspace = workspace;
                        self.recovery_error = None;
                        self.fallback_playhead = 0.0;
                        self.restart_audio();
                        self.notify_info("New project created.".into());
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            UiIntent::OpenProject => {
                self.spawn_dialog(DialogAction::OpenProject, || {
                    rfd::FileDialog::new()
                        .set_title("Open Notwig project bundle")
                        .pick_folder()
                });
            }
            UiIntent::Save => {
                self.sync_playhead();
                match self.workspace.save(None) {
                    Ok(path) => self.notify_info(format!("Saved {}", path.display())),
                    Err(_err) if self.workspace.bundle.is_none() => {
                        self.handle_save_as();
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            UiIntent::SaveAs => self.handle_save_as(),
            UiIntent::RenderMixdownRange { start, end } => {
                self.spawn_dialog(DialogAction::Render { start, end }, || {
                    rfd::FileDialog::new()
                        .add_filter("WAV", &["wav"])
                        .set_file_name("mixdown.wav")
                        .set_title("Render mixdown")
                        .save_file()
                });
            }
            UiIntent::Undo => {
                let result = self.workspace.undo();
                if self.report(result).is_some() {
                    self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                }
            }
            UiIntent::Redo => {
                let result = self.workspace.redo();
                if self.report(result).is_some() {
                    self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                }
            }
            UiIntent::ClearNotifications => self.ui.state.notifications.clear(),
        }
    }

    fn handle_save_as(&mut self) {
        self.sync_playhead();
        let file_name = format!("{}.notwig", self.workspace.doc.meta.name);
        self.spawn_dialog(DialogAction::SaveAs, move || {
            rfd::FileDialog::new()
                .set_file_name(file_name)
                .set_title("Save Notwig project bundle")
                .save_file()
        });
    }

    fn spawn_dialog<F>(&mut self, action: DialogAction, open: F)
    where
        F: FnOnce() -> Option<PathBuf> + Send + 'static,
    {
        if self.dialog_rx.is_some() {
            self.notify_info("A file dialog is already open.".into());
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send((action, open()));
        });
        self.dialog_rx = Some(receiver);
    }

    fn poll_dialog(&mut self) {
        if let Some(receiver) = &self.dialog_rx {
            match receiver.try_recv() {
                Ok((action, path)) => {
                    self.dialog_rx = None;
                    self.apply_dialog(action, path);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.dialog_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn apply_dialog(&mut self, action: DialogAction, path: Option<PathBuf>) {
        match action {
            DialogAction::OpenProject => {
                let Some(path) = path else {
                    return;
                };
                match Workspace::open(&path) {
                    Ok(workspace) => {
                        if self.workspace.bundle.as_deref() != Some(path.as_path()) {
                            self.workspace.end_session();
                        }
                        self.fallback_playhead = workspace.doc.transport.playhead.as_f64();
                        self.recovery_error = None;
                        self.workspace = workspace;
                        self.restart_audio();
                        self.notify_info(format!("Opened {}", path.display()));
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            DialogAction::SaveAs => {
                let Some(mut path) = path else {
                    return;
                };
                if path.extension().is_none() {
                    path.set_extension("notwig");
                }
                match self.workspace.save(Some(&path)) {
                    Ok(path) => {
                        self.notify_info(format!("Saved {}", path.display()));
                        let drops = std::mem::take(&mut self.pending_drops);
                        for (source, target, at) in drops {
                            self.perform_drop(source, target, at);
                        }
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            DialogAction::Render { start, end } => {
                let Some(path) = path else {
                    return;
                };
                match self.workspace.render_range_to(&path, start, end) {
                    Ok(frames) => self.notify_info(format!(
                        "Rendered {} frames to {}",
                        frames,
                        path.display()
                    )),
                    Err(err) => self.notify_error(err),
                }
            }
            DialogAction::ImportAudio => {
                let Some(path) = path else {
                    return;
                };
                let result = self.workspace.import_audio(&path);
                let _ = self.report(result);
            }
            DialogAction::Relink(asset_id) => {
                let Some(path) = path else {
                    return;
                };
                let result = self.workspace.relink_asset(asset_id, &path);
                let _ = self.report(result);
            }
        }
    }

    fn perform_drop(&mut self, path: PathBuf, target: DropAudioTarget, at: TimeMus) {
        if let DropAudioTarget::ClipEditor { clip_id } = target {
            let result = self.workspace.import_audio_slice(&path, clip_id, at);
            let _ = self.report(result);
            return;
        }
        let track_id = match target {
            DropAudioTarget::ClipEditor { .. } => None,
            DropAudioTarget::Track(track_id) => Some(track_id),
            DropAudioTarget::NewTrack { index } => {
                match self
                    .workspace
                    .create_track_at(notwig_core::TrackKind::Audio, Some(index))
                {
                    Ok(track_id) => Some(track_id),
                    Err(err) => {
                        self.notify_error(err);
                        None
                    }
                }
            }
        };
        if let Some(track_id) = track_id {
            let result = self.workspace.import_audio_at(&path, track_id, at);
            let _ = self.report(result);
        }
    }
}

impl eframe::App for NotwigApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.workspace.poll_engine_build();
        self.poll_dialog();
        self.workspace.poll_plugin_catalog();
        let mut editor_changes = Vec::new();
        for (device_id, window) in &mut self.plugin_editors {
            window.pump();
            for (param_id, value) in window.poll_changes() {
                editor_changes.push((*device_id, param_id, value));
            }
        }
        self.plugin_editors.retain(|(_, window)| window.is_open());
        for (device_id, raw_id, value) in editor_changes {
            let param_id =
                notwig_core::ParamId::from_uuid(uuid::Uuid::from_u128(u128::from(raw_id)));
            self.ui.last_touched_param.insert(device_id, param_id);
            let result = self.workspace.apply(CommandPayload::SetParam {
                device_id,
                param_id,
                value: notwig_core::ParamValue::F64(value),
                ramp: None,
            });
            let _ = self.report(result);
        }
        let dialog_open = self.dialog_rx.is_some();
        let dirty = if self.workspace.dirty { "*" } else { "" };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} — Notwig",
            self.workspace.doc.meta.name, dirty
        )));
        let snapshot = self.audio_snapshot();
        let playing = self.workspace.doc.transport.playing;
        if snapshot.running {
            self.last_tick = Instant::now();
            self.fallback_playhead = self
                .workspace
                .doc
                .tempo_map()
                .samples_to_mus(snapshot.playhead_samples)
                .as_f64();
        } else {
            self.advance_fallback(playing);
        }
        if let Some(error) = &snapshot.error {
            if self.workspace.status.as_deref() != Some(error.as_str()) {
                self.workspace.status = Some(error.clone());
            }
        }
        let runtime = self.runtime_view(&snapshot);
        self.ui.set_platform_pointer(platform_pointer(ctx));
        for spec in self.workspace.plugin_specs() {
            self.ui.registry.register(spec.clone());
        }
        let intents = self.ui.show(ctx, &self.workspace.doc, &runtime);
        for intent in intents {
            if matches!(intent, UiIntent::Quit) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.handle_intent(intent);
            }
        }
        self.show_recovery_dialog(ctx);
        if self.ui.take_shortcuts_dirty() {
            let result = crate::shortcuts::save(self.ui.shortcuts());
            let _ = self.report(result);
        }
        self.workspace.autosave_if_due();
        if dialog_open {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if playing {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if snapshot.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.workspace.engine_building() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn on_exit(&mut self) {
        self.workspace.end_session();
        if let Err(err) = crate::audio_config::save(&self.audio_settings) {
            tracing::warn!(error = %err, "could not save audio settings on exit");
        }
    }
}

fn f64_to_time(value: f64) -> TimeMus {
    TimeMus::new((value.max(0.0) * 1_000_000.0).round() as i64, 1_000_000)
}

#[allow(dead_code)]
fn _keep(_: &ProjectDoc, _: Rational, _: DeviceKind, _: TrackKind) {}

fn platform_pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
    let inner = ctx.input(|input| input.viewport().inner_rect)?;
    match Mouse::get_mouse_position() {
        Mouse::Position { x, y } => {
            let position = egui::pos2(x as f32, y as f32);
            if inner.contains(position) {
                Some(position - inner.min.to_vec2())
            } else {
                None
            }
        }
        Mouse::Error => None,
    }
}

fn audio_pick_dialog(title: &'static str) -> impl FnOnce() -> Option<PathBuf> + Send + 'static {
    move || {
        rfd::FileDialog::new()
            .add_filter(
                "Audio",
                &[
                    "wav", "wave", "aif", "aiff", "aifc", "mp3", "flac", "ogg", "oga",
                ],
            )
            .set_title(title)
            .pick_file()
    }
}
