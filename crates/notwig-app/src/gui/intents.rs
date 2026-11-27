use std::path::PathBuf;

use crate::workspace::Workspace;
use notwig_core::protocol::CommandPayload;
use notwig_ext_api::{CommandFocus, CommandUiRequest};
use notwig_ui::UiIntent;

#[cfg(not(target_arch = "wasm32"))]
use super::dialogs::DialogAction;
#[cfg(not(target_arch = "wasm32"))]
use super::platform::{audio_pick_dialog, pick_file, pick_folder, save_file};
#[cfg(target_arch = "wasm32")]
use super::render::{RenderJob, export_options};

use super::NotwigApp;

impl NotwigApp {
    pub(super) fn handle_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::Play => {
                let was_playing = self.workspace.doc.transport.playing;
                let play_start = self.workspace.doc.transport.playhead;
                let result = self.workspace.play();
                if self.report(result).is_some()
                    && self.workspace.doc.transport.recording
                    && self.recording.is_none()
                {
                    self.begin_recording_capture((!was_playing).then_some(play_start));
                }
            }
            UiIntent::Pause => {
                self.disengage_recording();
                self.sync_playhead();
                let result = self.workspace.pause();
                let _ = self.report(result);
            }
            UiIntent::Stop => {
                self.disengage_recording();
                let result = self.workspace.stop();
                let _ = self.report(result);
                self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
            }
            UiIntent::ToggleRecord => self.toggle_record(),
            UiIntent::ToggleLoop => {
                let result = self.workspace.toggle_loop();
                let _ = self.report(result);
            }
            UiIntent::ToggleMetronome => {
                let enabled = !self.workspace.doc.transport.metronome_enabled;
                let result = self.workspace.apply(CommandPayload::SetTransport {
                    playing: None,
                    playhead: None,
                    loop_enabled: None,
                    loop_range: None,
                    metronome: Some(enabled),
                });
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
                    metronome: None,
                });
                let _ = self.report(result);
            }
            UiIntent::RunExtensionCommand {
                command_id,
                context,
            } => self.run_extension_command(&command_id, &context),
            UiIntent::ApplyExtensionEffect { revision, effect } => {
                self.apply_extension_effect(revision, effect);
            }
            UiIntent::Quit
            | UiIntent::SelectTrack(_)
            | UiIntent::SelectClip(_)
            | UiIntent::SelectDevice(_) => {}
            UiIntent::CreateTrack(kind) => match self.workspace.create_track(kind) {
                Ok(track) => {
                    self.ui.state.selection.select_track(track, false);
                }
                Err(err) => self.notify_error(err),
            },
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
                match self.workspace.create_midi_clip(track_id, range) {
                    Ok(clip_id) => self
                        .ui
                        .state
                        .selection
                        .select_clip(clip_id, track_id, false),
                    Err(err) => self.notify_error(err),
                }
            }
            UiIntent::CreateAudioClip { track_id, range } => {
                match self.workspace.create_audio_clip(track_id, range) {
                    Ok(clip_id) => self
                        .ui
                        .state
                        .selection
                        .select_clip(clip_id, track_id, false),
                    Err(err) => self.notify_error(err),
                }
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
            UiIntent::SetClipGain { clip_id, gain_db } => {
                let result = self.workspace.apply(CommandPayload::EditClip {
                    clip_id,
                    range: None,
                    name: None,
                    muted: None,
                    content_start: None,
                    content_length: None,
                    audio: Some(notwig_core::protocol::AudioEdit {
                        source_start_sample: None,
                        source_end_sample: None,
                        gain_db: Some(gain_db),
                        fade_in_samples: None,
                        fade_out_samples: None,
                    }),
                });
                let _ = self.report(result);
            }
            UiIntent::SetClipColor { clip_id, color } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetClipColor { clip_id, color });
                let _ = self.report(result);
            }
            UiIntent::RenameClip { clip_id, name } => {
                let result = self.workspace.apply(CommandPayload::EditClip {
                    clip_id,
                    range: None,
                    name: Some(Some(name)),
                    muted: None,
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
                    tracing::debug!(count, "copied selection");
                    self.notify_info(format!("Copied {count} clip(s)."));
                } else {
                    self.notify_error("Nothing to copy.".into());
                }
            }
            UiIntent::CutSelection { clips, time } => {
                let count = self.workspace.cut_selection(&clips, time.clone());
                if count > 0 {
                    tracing::debug!(count, "cut selection");
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
                    tracing::debug!(count, "pasted selection");
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
            UiIntent::SetTrackRecordArm { track_id, armed } => {
                let result = self.workspace.apply(CommandPayload::SetTrackRecordArm {
                    id: track_id,
                    armed,
                });
                if result.is_ok() {
                    self.sync_recording_inputs();
                }
                let _ = self.report(result);
            }
            UiIntent::SetTrackInput { track_id, input } => {
                let result = self.workspace.apply(CommandPayload::SetTrackInput {
                    id: track_id,
                    input,
                });
                if result.is_ok() {
                    self.sync_recording_inputs();
                }
                let _ = self.report(result);
            }
            UiIntent::SetTrackMonitorMode { track_id, mode } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetTrackMonitorMode { id: track_id, mode });
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
            UiIntent::JoinClips { clips } => match self.workspace.join_clips(&clips) {
                Ok(info) => {
                    self.ui.state.selection.apply_info(&info);
                    self.notify_info("Joined clips.".into());
                }
                Err(err) => self.notify_error(err),
            },
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
            UiIntent::SetClipGainCurve { clip_id, curve } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetClipGainCurve { clip_id, curve });
                let _ = self.report(result);
            }
            UiIntent::PreviewClipGainCurve { clip_id, curve } => {
                let result = self.workspace.preview_clip_gain_curve(clip_id, curve);
                let _ = self.report(result);
            }
            UiIntent::SetClipPanCurve { clip_id, curve } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetClipPanCurve { clip_id, curve });
                let _ = self.report(result);
            }
            UiIntent::PreviewClipPanCurve { clip_id, curve } => {
                let result = self.workspace.preview_clip_pan_curve(clip_id, curve);
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
                length_mus,
            } => self
                .workspace
                .preview_note(track_id, note_id, pitch, velocity, length_mus),
            UiIntent::ReleasePreviewNote { track_id, note_id } => {
                self.workspace.release_preview_note(track_id, note_id);
            }
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
            UiIntent::InsertDevice {
                chain_id,
                type_id,
                version,
                kind,
                index,
                ..
            } => {
                let result = self
                    .workspace
                    .insert_device(chain_id, &type_id, version, kind, index);
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
            UiIntent::AnalyzeAudioDuration(path) => {
                if !self.audio_durations.contains_key(&path)
                    && self.audio_analysis_pending.insert(path.clone())
                {
                    let _ = self.audio_analysis_tx.send(path);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::ImportAudio => {
                self.spawn_dialog(DialogAction::ImportAudio, audio_pick_dialog("Import audio"));
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::ImportAudio => self.web_import_audio(),
            UiIntent::DropAudio { path, target, at } => {
                self.perform_drop(path, target, at);
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::DropAudioBytes {
                name,
                bytes,
                target,
                at,
            } => self.perform_drop_bytes(name, bytes, target, at),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::DropAudioBytes { .. } => {}
            UiIntent::InsertAudioClip { track_id, asset_id } => {
                let result = self.workspace.insert_audio_clip(track_id, asset_id);
                let _ = self.report(result);
            }
            UiIntent::RemoveDevice(device_id) => {
                let result = self
                    .workspace
                    .apply(CommandPayload::RemoveDevice { device_id });
                if result.is_ok() {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(audio) = &self.audio {
                        audio.close_editor(device_id);
                    }
                }
                let _ = self.report(result);
            }
            UiIntent::OpenPluginWindow { device_id } => self.open_plugin_window(device_id),
            UiIntent::MoveDevice {
                device_id,
                to_chain,
                index,
            } => {
                let result = self.workspace.move_device(device_id, to_chain, index);
                let _ = self.report(result);
            }
            UiIntent::CopyDevices(device_ids) => {
                self.workspace.copy_devices(&device_ids);
            }
            UiIntent::CutDevices(device_ids) => {
                let count = self.workspace.cut_devices(&device_ids);
                if count > 0 {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(audio) = &self.audio {
                        for device_id in &device_ids {
                            audio.close_editor(*device_id);
                        }
                    }
                }
            }
            UiIntent::PasteDevices {
                track_id,
                chain_id,
                index,
            } => match self.workspace.paste_devices(chain_id, index) {
                Ok(device_ids) => {
                    for (offset, device_id) in device_ids.iter().copied().enumerate() {
                        self.ui
                            .state
                            .selection
                            .select_device(device_id, track_id, offset > 0);
                    }
                }
                Err(err) => self.notify_error(err),
            },
            UiIntent::SetDeviceBypass { device_id, bypass } => {
                let result = self
                    .workspace
                    .apply(CommandPayload::SetDeviceBypass { device_id, bypass });
                let _ = self.report(result);
            }
            UiIntent::SetDevicesActive { device_ids, active } => {
                let result = self.workspace.set_devices_active(&device_ids, active);
                if result.is_ok() && !active {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(audio) = &self.audio {
                        for device_id in &device_ids {
                            audio.close_editor(*device_id);
                        }
                    }
                }
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
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(value) = raw_value
                    && let Some(audio) = &self.audio
                {
                    audio.set_editor_param(device_id, param_id, value);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = raw_value;
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::RelinkAsset(asset_id) => {
                self.spawn_dialog(
                    DialogAction::Relink(asset_id),
                    audio_pick_dialog("Relink audio asset"),
                );
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::RelinkAsset(_asset_id) => {
                self.notify_error("Asset relinking is not available in the browser.".into());
            }
            UiIntent::ApplyAudioSettings {
                driver,
                device,
                sample_rate,
                block_size,
                low_latency_block_size,
                low_latency_monitoring,
                channels,
                master_output,
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
                    low_latency_block_size,
                    low_latency_monitoring,
                    channels,
                    master_output,
                };
                self.workspace.audio_block_size = if block_size == 0 { 512 } else { block_size };
                self.workspace
                    .set_low_latency_monitoring(low_latency_monitoring);
                self.workspace.request_engine_rebuild();
                self.audio_options = super::platform::available_audio_options();
                self.restart_audio();
                if let Err(err) = crate::audio_config::save(&self.audio_settings) {
                    self.notify_error(format!("Audio settings applied but not saved: {err}"));
                } else {
                    self.notify_info("Audio settings applied.".into());
                }
            }
            UiIntent::NewProject | UiIntent::CloseProject => {
                let closed = matches!(intent, UiIntent::CloseProject);
                match Workspace::new_empty() {
                    Ok(workspace) => {
                        self.workspace.end_session();
                        self.workspace = workspace;
                        self.recovery_error = None;
                        self.fallback_playhead = 0.0;
                        #[cfg(target_arch = "wasm32")]
                        {
                            self.web_project = None;
                            self.web_picker = None;
                            self.sync_extensions();
                        }
                        self.restart_audio();
                        if closed {
                            self.notify_info("Project closed.".into());
                        } else {
                            self.notify_info("New project created.".into());
                        }
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::OpenProject => {
                self.spawn_dialog(DialogAction::OpenProject, || {
                    pick_folder("Open Notwig project bundle")
                });
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::OpenProject => self.web_open_project(),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::OpenProjectFolder => {
                self.spawn_dialog(DialogAction::OpenProject, || {
                    pick_folder("Open Notwig project bundle")
                });
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::OpenProjectFolder => self.web_open_project_folder(),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::OpenProjectNamed(key) => {
                self.apply_dialog(DialogAction::OpenProject, Some(PathBuf::from(key)));
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::OpenProjectNamed(name) => self.web_open_project_named(name),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::Save => {
                self.sync_playhead();
                match self.workspace.save(None) {
                    Ok(path) => {
                        self.refresh_recent_projects();
                        self.notify_info(format!("Saved {}", path.display()));
                    }
                    Err(_err) if self.workspace.bundle.is_none() => {
                        self.handle_save_as();
                    }
                    Err(err) => self.notify_error(err),
                }
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::Save => {
                self.sync_playhead();
                self.web_save_project(false);
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::SaveAs => self.handle_save_as(),
            #[cfg(target_arch = "wasm32")]
            UiIntent::SaveAs => {
                self.sync_playhead();
                self.web_save_project(true);
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::SaveProjectNamed(_) => {
                self.notify_error("Named project saves are only available in the browser.".into());
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::SaveProjectNamed(name) => self.web_save_project_named(name),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::CancelProjectPicker => {}
            #[cfg(target_arch = "wasm32")]
            UiIntent::CancelProjectPicker => self.web_picker = None,
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::DownloadProject => {
                self.notify_error("Project downloading is only available in the browser.".into());
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::DownloadProject => self.web_download_project(),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::RenderMixdownRange {
                start,
                end,
                options,
            } => {
                if self.render_busy() {
                    self.notify_info("A render is already running.".into());
                    return;
                }
                let extension = options.format.extension();
                if options.stems {
                    self.spawn_dialog(
                        DialogAction::Render {
                            start,
                            end,
                            options,
                        },
                        || pick_folder("Choose stem output folder"),
                    );
                } else {
                    self.spawn_dialog(
                        DialogAction::Render {
                            start,
                            end,
                            options,
                        },
                        move || {
                            save_file(
                                "Render mixdown",
                                &format!("mixdown.{extension}"),
                                &extension.to_uppercase(),
                                &[extension],
                            )
                        },
                    );
                }
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::RenderMixdownRange {
                start,
                end,
                options,
            } => {
                if self.render_busy() {
                    self.notify_info("A render is already running.".into());
                    return;
                }
                if options.stems {
                    self.notify_error("Stem rendering is not available in the browser yet.".into());
                    return;
                }
                if options.format != notwig_ui::ExportFormat::Wav {
                    self.notify_error("The browser build currently renders WAV only.".into());
                    return;
                }
                match self.workspace.begin_render(start, end) {
                    Ok(session) => {
                        self.ui.render_progress = Some(0.0);
                        let capacity = session.total_frames().saturating_mul(2);
                        self.render_job = Some(RenderJob {
                            session,
                            output: Vec::with_capacity(capacity),
                            path: PathBuf::from("mixdown.wav"),
                            options: export_options(options),
                        });
                    }
                    Err(error) => self.notify_error(error),
                }
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
            UiIntent::RestartAudioEngine => {
                self.engine_crash_reported = false;
                let was_lost = self.audio_device_lost;
                self.restart_audio();
                if self.audio.is_some() && !was_lost {
                    self.notify_info("Audio engine restarted.".into());
                }
            }
            UiIntent::StopAudioEngine => {
                if self.workspace.doc.transport.playing {
                    let _ = self.workspace.stop();
                }
                self.audio = None;
                #[cfg(not(target_arch = "wasm32"))]
                self.workspace.set_engine_process(None);
                self.engine_stopped = true;
                self.workspace.status = Some("Audio engine stopped.".into());
                self.notify_info("Audio engine stopped.".into());
            }
            UiIntent::CancelRender => {
                if self.render_job.take().is_some() {
                    self.ui.render_progress = None;
                    self.render_playhead = None;
                    self.notify_info("Render cancelled.".into());
                }
            }
            UiIntent::SetExtensionEnabled { id, enabled } => {
                self.set_extension_enabled(&id, enabled);
            }
            UiIntent::ReloadExtensions => {
                self.reload_extensions();
            }
            UiIntent::SetExtensionIndexes { indexes } => {
                self.set_extension_indexes(indexes);
            }
            UiIntent::RefreshExtensionIndex => {
                self.refresh_extension_index();
            }
            UiIntent::InstallExtensionFromIndex { id } => {
                self.install_extension_from_index(&id);
            }
            UiIntent::InstallExtensionZip { path } => {
                self.install_extension_zip(path);
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::InstallExtensionZipBytes { name, bytes } => {
                self.install_extension_zip_bytes(name, &bytes);
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::InstallExtensionZipBytes { .. } => {}
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::ImportExtensionZip => {
                self.spawn_dialog(DialogAction::ImportExtensionZip, || {
                    pick_file("Import extension ZIP", "Notwig extension", &["zip"])
                });
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::ImportExtensionZip => self.web_import_extension_zip(),
            UiIntent::UninstallExtension { id } => {
                self.uninstall_extension(&id);
            }
            UiIntent::ClearNotifications => self.ui.state.notifications.clear(),
        }
    }

    fn apply_extension_effect(&mut self, revision: u64, effect: notwig_ext_api::CommandEffect) {
        if revision != self.workspace.doc.revision {
            self.notify_error("Extension edit is stale. Try again.".into());
            return;
        }
        match self.workspace.apply_command_effect(effect) {
            Ok(ui) => {
                for request in ui {
                    self.apply_command_ui(request);
                }
            }
            Err(error) => self.notify_error(format!("Extension edit failed: {error}")),
        }
    }

    fn run_extension_command(
        &mut self,
        command_id: &str,
        context: &notwig_ext_api::CommandContext,
    ) {
        if context.revision != self.workspace.doc.revision {
            self.notify_error("Command context is stale. Try again.".into());
            return;
        }
        let effect = match self.extensions.execute_command(command_id, context) {
            Ok(effect) => effect,
            Err(error) => {
                self.notify_error(format!("Command failed: {error}"));
                return;
            }
        };
        if context.revision != self.workspace.doc.revision {
            self.notify_error("Command context changed while it was running.".into());
            return;
        }
        match self.workspace.apply_command_effect(effect) {
            Ok(ui) => {
                for request in ui {
                    self.apply_command_ui(request);
                }
            }
            Err(error) => self.notify_error(format!("Command failed: {error}")),
        }
    }

    fn apply_command_ui(&mut self, request: CommandUiRequest) {
        match request {
            CommandUiRequest::Notify { message } => self.notify_info(message),
            CommandUiRequest::OpenBottomPanel { id } => {
                self.ui.open_extension_bottom_panel(id);
            }
            CommandUiRequest::FocusEditor { focus, clip_id } => match focus {
                CommandFocus::Timeline => {
                    self.ui.edit_focus = notwig_ui::EditFocus::Timeline;
                }
                CommandFocus::MidiEditor => {
                    if let Some(clip_id) = clip_id {
                        self.ui.open_midi_editor(clip_id);
                    }
                }
                CommandFocus::AudioEditor => {
                    if let Some(clip_id) = clip_id {
                        self.ui.open_clip_editor(clip_id);
                    }
                }
                CommandFocus::DeviceEditor => {
                    self.ui.currently_opened = Some(notwig_ui::OpenedEditor::DeviceEditor);
                }
                CommandFocus::Panel => {}
            },
        }
    }
}
