mod audio;
mod dialogs;
mod extensions;
mod intents;
mod platform;
mod plugin_windows;
mod recording;
mod render;

use crate::clock::{Duration, Instant};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

#[cfg(target_arch = "wasm32")]
use crate::audio::AudioService;
#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::{EngineClient, EngineEvent};
use eframe::egui;
use notwig_core::protocol::CommandPayload;
use notwig_core::{AssetId, DeviceId, DeviceKind, ProjectDoc, Rational, TimeMus, TrackKind};
use notwig_ui::{DropAudioTarget, NotwigUi, RecentProjectView, RuntimeView, UiIntent};

#[cfg(target_arch = "wasm32")]
use crate::web::storage::{ProjectHandle, StorageEvent};
use crate::workspace::Workspace;

use audio::{AUDIO_RESTART_INITIAL_DELAY, AudioImportRequest, MeterSmoother};
use dialogs::DialogAction;
#[cfg(target_arch = "wasm32")]
use platform::WebProjectPicker;
#[cfg(not(target_arch = "wasm32"))]
use platform::platform_pointer;
use platform::{available_audio_options, load_recent_projects};
use recording::{ActiveRecording, append_captures};
use render::RenderJob;
#[cfg(not(target_arch = "wasm32"))]
use render::{RenderEncodeResult, StemsRenderResult};

#[cfg(not(target_arch = "wasm32"))]
pub fn run(initial: Option<PathBuf>) -> Result<(), String> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../../brand/notwig-app-icon.png"))
            .expect("embedded app icon should be a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Notwig")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(icon)
            // Let the egui app bar own the title area while keeping the
            // native macOS traffic lights available for window control.
            .with_title_shown(false)
            .with_titlebar_shown(false)
            .with_fullsize_content_view(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Notwig",
        options,
        Box::new(move |cc| {
            notwig_ui::configure(&cc.egui_ctx);
            Ok(Box::new(NotwigApp::new(initial)))
        }),
    )
    .map_err(|err| err.to_string())
}

pub struct NotwigApp {
    workspace: Workspace,
    ui: NotwigUi,
    extensions: crate::extensions::ExtensionManager,
    #[cfg(not(target_arch = "wasm32"))]
    audio: Option<EngineClient>,
    #[cfg(target_arch = "wasm32")]
    audio: Option<AudioService>,
    audio_settings: crate::audio::AudioSettings,
    audio_options: Vec<notwig_ui::AudioDriverView>,
    recording: Option<ActiveRecording>,
    fallback_playhead: f64,
    meter_smoother: MeterSmoother,
    last_tick: Instant,
    dialog_rx: Option<mpsc::Receiver<(DialogAction, Option<PathBuf>)>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pending_drops: Vec<(PathBuf, DropAudioTarget, TimeMus)>,
    #[cfg(not(target_arch = "wasm32"))]
    open_plugin_editors: std::collections::BTreeSet<DeviceId>,
    recovery_error: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    recovery_selection: Option<PathBuf>,
    render_job: Option<RenderJob>,
    #[cfg(target_arch = "wasm32")]
    pending_web_encode: Option<RenderJob>,
    #[cfg(not(target_arch = "wasm32"))]
    render_encode_rx: Option<mpsc::Receiver<RenderEncodeResult>>,
    #[cfg(not(target_arch = "wasm32"))]
    stems_render_rx: Option<mpsc::Receiver<StemsRenderResult>>,
    render_playhead: Option<TimeMus>,
    audio_durations: std::collections::BTreeMap<PathBuf, Option<f64>>,
    audio_analysis_pending: std::collections::BTreeSet<PathBuf>,
    audio_analysis_tx: mpsc::Sender<PathBuf>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    audio_analysis_rx: mpsc::Receiver<(PathBuf, Option<f64>)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    audio_import_tx: mpsc::Sender<AudioImportRequest>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    audio_import_rx: mpsc::Receiver<(AssetId, Result<notwig_project::ImportedAsset, String>)>,
    engine_crash_reported: bool,
    engine_stopped: bool,
    audio_device_lost: bool,
    audio_device_lost_message: Option<String>,
    next_audio_device_scan: Instant,
    audio_restart_at: Option<Instant>,
    audio_restart_backoff: Duration,
    recent_projects: Vec<RecentProjectView>,
    #[cfg(target_arch = "wasm32")]
    web_project: Option<ProjectHandle>,
    #[cfg(target_arch = "wasm32")]
    web_storage_rx: Option<mpsc::Receiver<StorageEvent>>,
    #[cfg(target_arch = "wasm32")]
    web_picker: Option<WebProjectPicker>,
    #[cfg(target_arch = "wasm32")]
    web_picker_epoch: u64,
}

impl NotwigApp {
    #[cfg(not(target_arch = "wasm32"))]
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
        if initial.is_none() {
            workspace.load_startup_recovery_candidates();
        }
        let fallback_playhead = workspace.doc.transport.playhead.as_f64();
        let audio_options = available_audio_options();
        let mut audio_settings =
            crate::audio_config::validated(crate::audio_config::load(), &audio_options);
        if audio_settings.block_size == 0 {
            audio_settings.block_size = workspace.audio_block_size;
        }
        let mut rebuild =
            workspace.set_low_latency_monitoring(audio_settings.low_latency_monitoring);
        if workspace.audio_block_size != audio_settings.block_size {
            workspace.audio_block_size = audio_settings.block_size;
            rebuild = true;
        }
        if rebuild {
            workspace.request_engine_rebuild();
        }
        let (audio_import_tx, audio_import_requests) = mpsc::channel::<AudioImportRequest>();
        let (audio_import_results, audio_import_rx) =
            mpsc::channel::<(AssetId, Result<notwig_project::ImportedAsset, String>)>();
        std::thread::spawn(move || {
            while let Ok(request) = audio_import_requests.recv() {
                let result = notwig_project::import_asset(
                    &request.source,
                    &request.target,
                    request.asset_id,
                    request.project_rate,
                )
                .map_err(|err| err.to_string());
                if audio_import_results
                    .send((request.asset_id, result))
                    .is_err()
                {
                    break;
                }
            }
        });
        let (audio_analysis_tx, audio_analysis_requests) = mpsc::channel::<PathBuf>();
        let (audio_analysis_results, audio_analysis_rx) = mpsc::channel::<(PathBuf, Option<f64>)>();
        std::thread::spawn(move || {
            while let Ok(path) = audio_analysis_requests.recv() {
                let duration = notwig_project::probe_audio(&path)
                    .ok()
                    .map(|probe| probe.duration_seconds)
                    .or_else(|| {
                        notwig_project::decode_audio(&path).ok().map(|buffer| {
                            buffer.frame_count() as f64 / f64::from(buffer.sample_rate.max(1))
                        })
                    });
                if audio_analysis_results.send((path, duration)).is_err() {
                    break;
                }
            }
        });
        let extensions = crate::extensions::ExtensionManager::load();
        let mut app = Self {
            workspace,
            ui: NotwigUi::default(),
            extensions,
            audio: None,
            audio_settings,
            audio_options,
            recording: None,
            fallback_playhead,
            meter_smoother: MeterSmoother::default(),
            last_tick: Instant::now(),
            dialog_rx: None,
            pending_drops: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            open_plugin_editors: std::collections::BTreeSet::new(),
            recovery_error: None,
            #[cfg(not(target_arch = "wasm32"))]
            recovery_selection: None,
            render_job: None,
            #[cfg(not(target_arch = "wasm32"))]
            render_encode_rx: None,
            #[cfg(not(target_arch = "wasm32"))]
            stems_render_rx: None,
            render_playhead: None,
            audio_durations: std::collections::BTreeMap::new(),
            audio_analysis_pending: std::collections::BTreeSet::new(),
            audio_analysis_tx,
            audio_analysis_rx,
            audio_import_tx,
            audio_import_rx,
            engine_crash_reported: false,
            engine_stopped: false,
            audio_device_lost: false,
            audio_device_lost_message: None,
            next_audio_device_scan: Instant::now(),
            audio_restart_at: None,
            audio_restart_backoff: AUDIO_RESTART_INITIAL_DELAY,
            recent_projects: load_recent_projects(),
        };
        app.ui.set_shortcuts(crate::shortcuts::load());
        app.sync_extensions();
        app.restart_audio();
        app
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_web() -> Self {
        let workspace = Workspace::new_empty().expect("create fallback workspace");
        let fallback_playhead = workspace.doc.transport.playhead.as_f64();
        let (audio_import_tx, _audio_import_requests) = mpsc::channel::<AudioImportRequest>();
        let (audio_analysis_tx, _audio_analysis_requests) = mpsc::channel::<PathBuf>();
        let (_audio_analysis_results, audio_analysis_rx) =
            mpsc::channel::<(PathBuf, Option<f64>)>();
        let (_audio_import_results, audio_import_rx) =
            mpsc::channel::<(AssetId, Result<notwig_project::ImportedAsset, String>)>();
        let mut app = Self {
            workspace,
            ui: NotwigUi::default(),
            extensions: crate::extensions::ExtensionManager::load(),
            audio: None,
            audio_settings: crate::audio::AudioSettings {
                block_size: 512,
                low_latency_monitoring: cfg!(not(target_arch = "wasm32")),
                ..crate::audio::AudioSettings::default()
            },
            audio_options: available_audio_options(),
            recording: None,
            fallback_playhead,
            meter_smoother: MeterSmoother::default(),
            last_tick: Instant::now(),
            dialog_rx: None,
            pending_drops: Vec::new(),
            recovery_error: None,
            render_job: None,
            #[cfg(target_arch = "wasm32")]
            pending_web_encode: None,
            render_playhead: None,
            audio_durations: std::collections::BTreeMap::new(),
            audio_analysis_pending: std::collections::BTreeSet::new(),
            audio_analysis_tx,
            audio_analysis_rx,
            audio_import_tx,
            audio_import_rx,
            engine_crash_reported: false,
            engine_stopped: false,
            audio_device_lost: false,
            audio_device_lost_message: None,
            next_audio_device_scan: Instant::now(),
            audio_restart_at: None,
            audio_restart_backoff: AUDIO_RESTART_INITIAL_DELAY,
            recent_projects: load_recent_projects(),
            web_project: None,
            web_storage_rx: None,
            web_picker: None,
            web_picker_epoch: 0,
        };
        app.ui.set_shortcuts(crate::shortcuts::load());
        app.sync_extensions();
        app
    }

    pub fn configure(&mut self, ctx: &egui::Context) {
        notwig_ui::configure(ctx);
    }

    pub fn enable_audio(&mut self) {
        self.audio_options = available_audio_options();
        self.restart_audio();
    }

    fn runtime_view(&self, snapshot: &crate::audio::AudioSnapshot) -> RuntimeView {
        let fallback_time = f64_to_time(self.fallback_playhead);
        let playhead = if let Some(render_playhead) = self.render_playhead {
            render_playhead
        } else if snapshot.running {
            self.interpolated_playhead(snapshot)
        } else {
            fallback_time
        };
        let meters = self.meter_smoother.values().clone();
        let device_meters =
            snapshot.device_meters_for_display(self.workspace.doc.transport.playing);
        let recording_takes = self.recording_take_views(playhead);
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
        );
        #[cfg(target_arch = "wasm32")]
        let project_picker = self
            .web_picker
            .as_ref()
            .map(|picker| notwig_ui::ProjectPickerView {
                epoch: picker.epoch,
                action: picker.action,
                projects: picker.projects.clone(),
                loading: picker.loading,
                error: picker.error.clone(),
                suggested_name: picker.suggested_name.clone(),
            });
        #[cfg(not(target_arch = "wasm32"))]
        let project_picker = None;
        RuntimeView {
            playing: self.render_playhead.is_some() || self.workspace.doc.transport.playing,
            playhead,
            play_start: self
                .workspace
                .play_start
                .unwrap_or(self.workspace.doc.transport.playhead),
            waveforms: Arc::clone(&self.workspace.waveforms),
            audio_durations: self.audio_durations.clone(),
            audio_analysis_pending: self.audio_analysis_pending.clone(),
            meters,
            device_meters,
            xruns: snapshot.xruns,
            cpu_load: snapshot.cpu_load,
            status,
            engine_crashed: snapshot.crashed,
            engine_stopped: self.engine_stopped,
            audio_device_lost: self.audio_device_lost,
            audio_device_lost_message: self.audio_device_lost_message.clone(),
            plugins: self.workspace.plugins.clone(),
            bundle_path: self.workspace.bundle.clone(),
            dirty: self.workspace.dirty,
            saving: false,
            audio_settings,
            recording_takes,
            dsp: snapshot.dsp.clone(),
            clipboard: self.workspace.clipboard().clone(),
            extensions: self.extensions.infos(),
            extensions_error: self.extensions.error().map(str::to_owned),
            extension_errors: self.extensions.errors(),
            extension_icons: self.extensions.icons(),
            extension_indexes: self.extensions.indexes(),
            extension_index: self.extensions.index_entries(),
            extension_index_errors: self.extensions.index_errors(),
            extension_index_loading: self.extensions.index_loading(),
            extension_index_loaded: self.extensions.index_loaded(),
            recent_projects: self.recent_projects.clone(),
            project_picker,
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

    fn interpolated_playhead(&self, snapshot: &crate::audio::AudioSnapshot) -> TimeMus {
        if !(snapshot.running && snapshot.playing && self.workspace.doc.transport.playing) {
            return self.workspace.doc.transport.playhead;
        }
        let map = self.workspace.doc.tempo_map();
        let base = snapshot.playhead_samples as f64;
        let elapsed = snapshot
            .playhead_updated
            .map_or(0.0, |updated| updated.elapsed().as_secs_f64());
        let mut samples = base + elapsed * f64::from(snapshot.sample_rate.max(1));
        if self.workspace.doc.transport.loop_enabled
            && let Some(range) = self.workspace.doc.transport.loop_range
        {
            let start = map.mus_to_samples(range.start) as f64;
            let end = map.mus_to_samples(range.end) as f64;
            if end > start && base < end && samples >= end {
                samples = start + (samples - start) % (end - start);
            }
        }
        map.samples_to_mus(samples.max(0.0).round() as u64)
    }

    pub(super) fn current_playhead(&self, snapshot: &crate::audio::AudioSnapshot) -> TimeMus {
        if snapshot.running {
            self.interpolated_playhead(snapshot)
        } else if self.workspace.doc.transport.playing {
            f64_to_time(self.fallback_playhead)
        } else {
            self.workspace.doc.transport.playhead
        }
    }

    pub(super) fn sync_playhead(&mut self) {
        let snapshot = self.audio_snapshot();
        let playhead = self.current_playhead(&snapshot);
        let _ = self.workspace.sync_playhead_for_save(playhead);
        self.fallback_playhead = playhead.as_f64();
    }
}

impl Drop for NotwigApp {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                return;
            };
            for device_id in self.open_plugin_editors.iter().copied() {
                audio.close_editor(device_id);
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline && !self.open_plugin_editors.is_empty() {
                let events = audio.poll();
                for event in events {
                    if let EngineEvent::EditorClosed {
                        device_id,
                        state,
                        error,
                    } = event
                    {
                        self.open_plugin_editors.remove(&device_id);
                        if let Some(error) = error {
                            tracing::warn!(%error, "plugin editor closed with error during shutdown");
                        }
                        if let Some(state) = state {
                            self.workspace.store_plugin_state(device_id, state);
                        }
                    }
                }
                if self.open_plugin_editors.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

impl eframe::App for NotwigApp {
    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.workspace.poll_engine_build();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.poll_dialog();
            self.poll_audio_analysis();
            self.poll_audio_imports();
        }
        #[cfg(target_arch = "wasm32")]
        self.poll_web_encode();
        self.advance_render();
        #[cfg(target_arch = "wasm32")]
        self.poll_web_recording();
        #[cfg(target_arch = "wasm32")]
        if let Some(error) = self.audio.as_ref().and_then(AudioService::take_input_error) {
            self.notify_error(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.poll_render_encode();
        #[cfg(not(target_arch = "wasm32"))]
        self.poll_stems_render();
        #[cfg(target_arch = "wasm32")]
        self.poll_web_storage();
        self.workspace.poll_plugin_catalog();
        self.poll_extension_events(ctx);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let events = self
                .audio
                .as_ref()
                .map(EngineClient::poll)
                .unwrap_or_default();
            let mut editor_changes = Vec::new();
            let mut closed_states = Vec::new();
            let mut playback_toggle = false;
            let mut build_results = Vec::new();
            for event in events {
                match event {
                    EngineEvent::BuildFinished(result) => build_results.push(result),
                    EngineEvent::EditorOpened { device_id } => {
                        self.open_plugin_editors.insert(device_id);
                    }
                    EngineEvent::EditorClosed {
                        device_id,
                        state,
                        error,
                    } => {
                        if let Some(error) = error {
                            self.notify_error(error);
                        }
                        self.open_plugin_editors.remove(&device_id);
                        if let Some(state) = state {
                            closed_states.push((device_id, state));
                        }
                    }
                    EngineEvent::EditorParam {
                        device_id,
                        param_id,
                        value,
                    } => editor_changes.push((device_id, param_id, value)),
                    EngineEvent::EditorTransportToggle => playback_toggle = true,
                    EngineEvent::InputError(error) | EngineEvent::Error(error) => {
                        self.notify_error(error);
                    }
                    EngineEvent::RecordingChunk {
                        sample_rate,
                        captures,
                    } => self.append_recording_chunk(sample_rate, captures),
                    EngineEvent::RecordingFinished(captures) => {
                        let end = self
                            .recording
                            .as_ref()
                            .and_then(|active| active.end)
                            .unwrap_or_else(|| {
                                let snapshot = self.audio_snapshot();
                                self.current_playhead(&snapshot)
                            });
                        let Some(mut active) = self.recording.take() else {
                            continue;
                        };
                        let captures = captures
                            .into_iter()
                            .map(|(track_id, buffer)| (track_id, buffer.samples))
                            .collect();
                        append_captures(&mut active, captures);
                        self.finalize_live_recording(active, end);
                        if self.workspace.doc.transport.recording
                            && self.workspace.doc.transport.playing
                            && self.recording.is_none()
                        {
                            self.begin_recording_capture(None);
                        }
                    }
                    EngineEvent::RecordingFailed(error) => {
                        self.recording = None;
                        self.notify_error(error);
                    }
                }
            }
            for result in build_results {
                self.workspace.engine_process_build_finished(result);
            }
            if playback_toggle {
                let intent = if self.workspace.doc.transport.playing {
                    UiIntent::Stop
                } else {
                    UiIntent::Play
                };
                self.handle_intent(intent);
            }
            for (device_id, state) in closed_states {
                self.workspace.set_plugin_state(device_id, state);
            }
            for (device_id, param_id, value) in editor_changes {
                self.ui.last_touched_param.insert(device_id, param_id);
                let result = self.workspace.apply(CommandPayload::SetParam {
                    device_id,
                    param_id,
                    value: notwig_core::ParamValue::F64(value),
                    ramp: None,
                });
                let _ = self.report(result);
            }
        }
        let dialog_open = self.dialog_rx.is_some();
        let rendering = self.render_busy();
        let dirty = if self.workspace.dirty { "*" } else { "" };
        let title = format!("{}{} — Notwig", self.workspace.doc.meta.name, dirty);
        #[cfg(not(target_arch = "wasm32"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(&title);
        }
        let snapshot = self.audio_snapshot();
        #[cfg(not(target_arch = "wasm32"))]
        self.workspace
            .set_engine_playhead_samples(snapshot.playhead_samples);
        self.poll_audio_device_state(&snapshot);
        if snapshot.crashed && !self.engine_crash_reported {
            self.engine_crash_reported = true;
            self.notify_error(
                "Audio engine crashed. Restart the engine to resume playback.".into(),
            );
        } else if !snapshot.crashed {
            self.engine_crash_reported = false;
        }
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
        self.refresh_recording_waveforms();
        self.meter_smoother.update(
            &snapshot.meters_for_display(self.workspace.doc.transport.playing),
            Instant::now(),
        );
        let runtime = self.runtime_view(&snapshot);
        #[cfg(not(target_arch = "wasm32"))]
        self.ui.set_platform_pointer(platform_pointer(ctx));
        #[cfg(target_arch = "wasm32")]
        self.ui.set_platform_pointer(
            crate::web::take_drag_pointer()
                .or_else(|| ctx.input(|input| input.pointer.latest_pos())),
        );
        for spec in self.workspace.plugin_specs() {
            self.ui.registry.register(spec.clone());
        }
        for spec in self.workspace.extension_specs() {
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
        if rendering {
            // An active render advances only when `update` runs, so drive it
            // continuously instead of following the idle repaint cadence.
            ctx.request_repaint();
        } else if playing {
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
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
