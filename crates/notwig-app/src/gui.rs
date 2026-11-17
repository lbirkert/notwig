use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use notwig_core::protocol::CommandPayload;
use notwig_core::{DeviceKind, ProjectDoc, Rational, TimeMus, TrackKind};
use notwig_ui::{MeterValues, NotificationKind, NotwigUi, RuntimeView, UiIntent};

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

struct NotwigApp {
    workspace: Workspace,
    ui: NotwigUi,
    audio: Option<AudioService>,
    fallback_playhead: f64,
    last_tick: Instant,
}

impl NotwigApp {
    fn new(initial: Option<PathBuf>) -> Self {
        let opened = match initial.as_deref() {
            Some(path) => Workspace::open(path),
            None => Workspace::new_empty(),
        };
        let workspace = match opened {
            Ok(workspace) => workspace,
            Err(err) => {
                let mut workspace = Workspace::new_empty().expect("create fallback workspace");
                workspace.status = Some(format!("Could not open project: {err}"));
                workspace
            }
        };
        let fallback_playhead = workspace.doc.transport.playhead.as_f64();
        let mut app = Self {
            workspace,
            ui: NotwigUi::default(),
            audio: None,
            fallback_playhead,
            last_tick: Instant::now(),
        };
        app.restart_audio();
        app
    }

    fn restart_audio(&mut self) {
        self.audio = None;
        let rate = self.workspace.doc.meta.sample_rate.value();
        match AudioService::start(Arc::clone(&self.workspace.engine), rate) {
            Ok(audio) => self.audio = Some(audio),
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
        let meters = if snapshot.running {
            snapshot.meters.clone()
        } else {
            std::collections::BTreeMap::<notwig_core::TrackId, MeterValues>::new()
        };
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
        RuntimeView {
            playing: self.workspace.doc.transport.playing,
            playhead,
            waveforms: self.workspace.peaks.clone(),
            meters,
            xruns: snapshot.xruns,
            cpu_load: None,
            status,
            plugins: self.workspace.plugins.clone(),
            bundle_path: self.workspace.bundle.clone(),
            dirty: self.workspace.dirty,
            saving: false,
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

    fn notify_info(&mut self, message: String) {
        self.ui
            .state
            .notify(NotificationKind::Info, message.clone());
        self.workspace.status = Some(message);
    }

    fn handle_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::Play => {
                let result = self.workspace.play();
                if let Err(err) = result {
                    self.notify_error(err);
                }
            }
            UiIntent::Pause => {
                self.sync_playhead();
                if let Err(err) = self.workspace.pause() {
                    self.notify_error(err);
                }
            }
            UiIntent::Stop => {
                if let Err(err) = self.workspace.stop() {
                    self.notify_error(err);
                }
                self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
            }
            UiIntent::ToggleLoop => {
                if let Err(err) = self.workspace.toggle_loop() {
                    self.notify_error(err);
                }
            }
            UiIntent::SetPlayhead(time) => {
                self.fallback_playhead = time.as_f64();
                if let Err(err) = self.workspace.set_playhead(time) {
                    self.notify_error(err);
                }
            }
            UiIntent::SetLoopRange(range) => {
                if let Err(err) = self.workspace.apply(CommandPayload::SetTransport {
                    playing: None,
                    playhead: None,
                    loop_enabled: Some(true),
                    loop_range: Some(Some(range)),
                }) {
                    self.notify_error(err);
                }
            }
            UiIntent::SelectTrack(_) | UiIntent::SelectClip(_) | UiIntent::SelectDevice(_) => {}
            UiIntent::CreateTrack(kind) => {
                if let Err(err) = self.workspace.create_track(kind) {
                    self.notify_error(err);
                }
            }
            UiIntent::DeleteTrack(track_id) => {
                if let Err(err) = self
                    .workspace
                    .apply(CommandPayload::DeleteTrack { id: track_id })
                {
                    self.notify_error(err);
                }
            }
            UiIntent::DeleteClip(clip_id) => {
                if let Err(err) = self.workspace.apply(CommandPayload::DeleteClip { clip_id }) {
                    self.notify_error(err);
                }
            }
            UiIntent::CreateMidiClip { track_id, range } => {
                if let Err(err) = self.workspace.create_midi_clip(track_id, range) {
                    self.notify_error(err);
                }
            }
            UiIntent::EditClipRange { clip_id, range } => {
                if let Err(err) = self.workspace.apply(CommandPayload::EditClip {
                    clip_id,
                    range: Some(range),
                    name: None,
                    muted: None,
                    content_start: None,
                    content_length: None,
                    audio: None,
                }) {
                    self.notify_error(err);
                }
            }
            UiIntent::MoveClip {
                clip_id,
                track_id,
                range,
            } => {
                if let Err(err) = self.workspace.apply(CommandPayload::MoveClip {
                    clip_id,
                    track_id: Some(track_id),
                    range,
                }) {
                    self.notify_error(err);
                }
            }
            UiIntent::SetClipMuted { clip_id, muted } => {
                if let Err(err) = self.workspace.apply(CommandPayload::EditClip {
                    clip_id,
                    range: None,
                    name: None,
                    muted: Some(muted),
                    content_start: None,
                    content_length: None,
                    audio: None,
                }) {
                    self.notify_error(err);
                }
            }
            UiIntent::DuplicateClip { clip_id } => {
                if let Err(err) = self.workspace.duplicate_clip(clip_id) {
                    self.notify_error(err);
                }
            }
            UiIntent::SplitClip { clip_id, at } => {
                if let Err(err) = self.workspace.split_clip(clip_id, at) {
                    self.notify_error(err);
                }
            }
            UiIntent::SetTrackMixer {
                track_id,
                volume_db,
                pan,
                mute,
                solo,
            } => {
                if let Err(err) = self
                    .workspace
                    .set_mixer(track_id, volume_db, pan, mute, solo)
                {
                    self.notify_error(err);
                }
            }
            UiIntent::InsertStockDevice {
                chain_id,
                type_id,
                kind,
                ..
            } => {
                if let Err(err) = self.workspace.insert_stock(chain_id, &type_id, kind) {
                    self.notify_error(err);
                }
            }
            UiIntent::InsertPlugin {
                chain_id,
                type_id,
                kind,
                ..
            } => {
                if let Err(err) = self.workspace.insert_plugin(chain_id, &type_id, kind) {
                    self.notify_error(err);
                }
            }
            UiIntent::ImportAudio => {
                if self.workspace.bundle.is_none() {
                    self.notify_error("Save the project before importing audio.".into());
                } else if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Audio", &["wav", "wave", "aif", "aiff", "aifc"])
                    .set_title("Import audio")
                    .pick_file()
                {
                    if let Err(err) = self.workspace.import_audio(&path) {
                        self.notify_error(err);
                    }
                }
            }
            UiIntent::DropAudio { path, track_id, at } => {
                if self.workspace.bundle.is_none() {
                    self.notify_error("Save the project before importing audio.".into());
                } else if let Err(err) = self.workspace.import_audio_at(&path, track_id, at) {
                    self.notify_error(err);
                }
            }
            UiIntent::InsertAudioClip { track_id, asset_id } => {
                if let Err(err) = self.workspace.insert_audio_clip(track_id, asset_id) {
                    self.notify_error(err);
                }
            }
            UiIntent::RemoveDevice(device_id) => {
                if let Err(err) = self
                    .workspace
                    .apply(CommandPayload::RemoveDevice { device_id })
                {
                    self.notify_error(err);
                }
            }
            UiIntent::SetDeviceBypass { device_id, bypass } => {
                if let Err(err) = self
                    .workspace
                    .apply(CommandPayload::SetDeviceBypass { device_id, bypass })
                {
                    self.notify_error(err);
                }
            }
            UiIntent::SetParam {
                device_id,
                param_id,
                value,
            } => {
                if let Err(err) = self.workspace.apply(CommandPayload::SetParam {
                    device_id,
                    param_id,
                    value,
                    ramp: None,
                }) {
                    self.notify_error(err);
                }
            }
            UiIntent::RelinkAsset(asset_id) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Audio", &["wav", "wave", "aif", "aiff", "aifc"])
                    .set_title("Relink audio asset")
                    .pick_file()
                {
                    if let Err(err) = self.workspace.relink_asset(asset_id, &path) {
                        self.notify_error(err);
                    }
                }
            }
            UiIntent::NewProject => match Workspace::new_empty() {
                Ok(workspace) => {
                    self.workspace = workspace;
                    self.fallback_playhead = 0.0;
                    self.restart_audio();
                    self.notify_info("New project created.".into());
                }
                Err(err) => self.notify_error(err),
            },
            UiIntent::OpenProject => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open Notwig project bundle")
                    .pick_folder()
                {
                    match Workspace::open(&path) {
                        Ok(workspace) => {
                            self.fallback_playhead = workspace.doc.transport.playhead.as_f64();
                            self.workspace = workspace;
                            self.restart_audio();
                            self.notify_info(format!("Opened {}", path.display()));
                        }
                        Err(err) => self.notify_error(err),
                    }
                }
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
            UiIntent::RenderMixdown => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("WAV", &["wav"])
                    .set_file_name("mixdown.wav")
                    .set_title("Render mixdown")
                    .save_file()
                {
                    match self.workspace.render_to(&path) {
                        Ok(frames) => self.notify_info(format!(
                            "Rendered {} frames to {}",
                            frames,
                            path.display()
                        )),
                        Err(err) => self.notify_error(err),
                    }
                }
            }
            UiIntent::ClearNotifications => self.ui.state.notifications.clear(),
        }
    }

    fn handle_save_as(&mut self) {
        self.sync_playhead();
        if let Some(mut path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.notwig", self.workspace.doc.meta.name))
            .set_title("Save Notwig project bundle")
            .save_file()
        {
            if path.extension().is_none() {
                path.set_extension("notwig");
            }
            match self.workspace.save(Some(&path)) {
                Ok(path) => self.notify_info(format!("Saved {}", path.display())),
                Err(err) => self.notify_error(err),
            }
        }
    }
}

impl eframe::App for NotwigApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        let intents = self.ui.show(ctx, &self.workspace.doc, &runtime);
        for intent in intents {
            self.handle_intent(intent);
        }
        self.workspace.autosave_if_due();
        if playing {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if snapshot.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

fn f64_to_time(value: f64) -> TimeMus {
    TimeMus::new((value.max(0.0) * 1_000_000.0).round() as i64, 1_000_000)
}

#[allow(dead_code)]
fn _keep(_: &ProjectDoc, _: Rational, _: DeviceKind, _: TrackKind) {}
