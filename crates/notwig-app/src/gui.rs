use crate::clock::{Duration, Instant};
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use mouse_position::mouse_position::Mouse;
use notwig_core::protocol::CommandPayload;
use notwig_core::{AssetId, DeviceId, DeviceKind, ProjectDoc, Rational, TimeMus, TrackKind};
#[cfg(not(target_arch = "wasm32"))]
use notwig_plugins::NativePluginWindow;
use notwig_ui::{DropAudioTarget, NotificationKind, NotwigUi, RuntimeView, UiIntent};

use crate::audio::AudioService;
#[cfg(target_arch = "wasm32")]
use crate::web::storage::{FilePurpose, ProjectHandle, StorageEvent};
use crate::workspace::{RenderSession, Workspace};

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug, Clone, Copy)]
enum DialogAction {
    OpenProject,
    SaveAs,
    Render {
        start: TimeMus,
        end: TimeMus,
        options: notwig_ui::ExportOptions,
    },
    ImportAudio,
    ImportExtensionZip,
    Relink(notwig_core::AssetId),
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct AudioImportRequest {
    asset_id: AssetId,
    source: PathBuf,
    target: PathBuf,
    project_rate: u32,
}

struct RenderJob {
    session: RenderSession,
    output: Vec<f32>,
    path: PathBuf,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    options: notwig_project::ExportOptions,
}

pub struct NotwigApp {
    workspace: Workspace,
    ui: NotwigUi,
    extensions: crate::extensions::ExtensionManager,
    audio: Option<AudioService>,
    audio_settings: crate::audio::AudioSettings,
    audio_options: Vec<notwig_ui::AudioDriverView>,
    fallback_playhead: f64,
    last_tick: Instant,
    dialog_rx: Option<mpsc::Receiver<(DialogAction, Option<PathBuf>)>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pending_drops: Vec<(PathBuf, DropAudioTarget, TimeMus)>,
    #[cfg(not(target_arch = "wasm32"))]
    plugin_editors: Vec<(DeviceId, NativePluginWindow)>,
    recovery_error: Option<String>,
    render_job: Option<RenderJob>,
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
    #[cfg(target_arch = "wasm32")]
    web_project: Option<ProjectHandle>,
    #[cfg(target_arch = "wasm32")]
    web_storage_rx: Option<mpsc::Receiver<StorageEvent>>,
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
        let fallback_playhead = workspace.doc.transport.playhead.as_f64();
        let audio_options = AudioService::available_options();
        let mut audio_settings =
            crate::audio_config::validated(crate::audio_config::load(), &audio_options);
        if audio_settings.block_size == 0 {
            audio_settings.block_size = workspace.audio_block_size;
        }
        if workspace.audio_block_size != audio_settings.block_size {
            workspace.audio_block_size = audio_settings.block_size;
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
                let duration = notwig_project::decode_audio(&path).ok().map(|buffer| {
                    buffer.frame_count() as f64 / f64::from(buffer.sample_rate.max(1))
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
            fallback_playhead,
            last_tick: Instant::now(),
            dialog_rx: None,
            pending_drops: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            plugin_editors: Vec::new(),
            recovery_error: None,
            render_job: None,
            render_playhead: None,
            audio_durations: std::collections::BTreeMap::new(),
            audio_analysis_pending: std::collections::BTreeSet::new(),
            audio_analysis_tx,
            audio_analysis_rx,
            audio_import_tx,
            audio_import_rx,
            engine_crash_reported: false,
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
                ..crate::audio::AudioSettings::default()
            },
            audio_options: Vec::new(),
            fallback_playhead,
            last_tick: Instant::now(),
            dialog_rx: None,
            pending_drops: Vec::new(),
            recovery_error: None,
            render_job: None,
            render_playhead: None,
            audio_durations: std::collections::BTreeMap::new(),
            audio_analysis_pending: std::collections::BTreeSet::new(),
            audio_analysis_tx,
            audio_analysis_rx,
            audio_import_tx,
            audio_import_rx,
            engine_crash_reported: false,
            web_project: None,
            web_storage_rx: None,
        };
        app.ui.set_shortcuts(crate::shortcuts::load());
        app.sync_extensions();
        app
    }

    pub fn configure(&mut self, _ctx: &egui::Context) {}

    pub fn enable_audio(&mut self) {
        self.restart_audio();
    }

    #[cfg(target_arch = "wasm32")]
    fn web_open_project(&mut self) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);

        if let Some(name) = crate::web::storage::last_project_name() {
            wasm_bindgen_futures::spawn_local(async move {
                let result = async {
                    let handle = crate::web::storage::open_opfs_project(&name).await?;
                    let files = crate::web::storage::read_bundle(&handle).await?;
                    Ok((handle, files))
                }
                .await;
                let event = match result {
                    Ok((handle, files)) => StorageEvent::ProjectLoaded { handle, files },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }

        if let Ok(promise) = crate::web::storage::request_directory() {
            wasm_bindgen_futures::spawn_local(async move {
                let result = async {
                    let handle = crate::web::storage::directory_from_promise(promise).await?;
                    let files = crate::web::storage::read_bundle(&handle).await?;
                    Ok((handle, files))
                }
                .await;
                let event = match result {
                    Ok((handle, files)) => StorageEvent::ProjectLoaded { handle, files },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }

        self.web_storage_rx = None;
        self.notify_error("No browser project has been saved yet.".into());
    }

    #[cfg(target_arch = "wasm32")]
    fn web_open_project_folder(&mut self) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let promise = match crate::web::storage::request_directory() {
            Ok(promise) => promise,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let handle = crate::web::storage::directory_from_promise(promise).await?;
                let files = crate::web::storage::read_bundle(&handle).await?;
                Ok((handle, files))
            }
            .await;
            let event = match result {
                Ok((handle, files)) => StorageEvent::ProjectLoaded { handle, files },
                Err(error) => StorageEvent::Error(error),
            };
            let _ = sender.send(event);
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn web_save_project(&mut self, choose_new: bool) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let files = match self.workspace.bundle_files() {
            Ok(files) => files,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);

        if let Some(handle) = self.web_project.clone().filter(|_| !choose_new) {
            wasm_bindgen_futures::spawn_local(async move {
                let event = match crate::web::storage::write_bundle(&handle, &files).await {
                    Ok(()) => StorageEvent::ProjectSaved { handle },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }

        if choose_new {
            if let Ok(promise) = crate::web::storage::request_directory() {
                wasm_bindgen_futures::spawn_local(async move {
                    let result = async {
                        let handle = crate::web::storage::directory_from_promise(promise).await?;
                        crate::web::storage::write_bundle(&handle, &files).await?;
                        Ok(handle)
                    }
                    .await;
                    let event = match result {
                        Ok(handle) => StorageEvent::ProjectSaved { handle },
                        Err(error) => StorageEvent::Error(error),
                    };
                    let _ = sender.send(event);
                });
                return;
            }
        }

        let name = sanitized_bundle_name(&self.workspace.doc.meta.name);
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let handle = crate::web::storage::open_opfs_project(&name).await?;
                crate::web::storage::write_bundle(&handle, &files).await?;
                Ok(handle)
            }
            .await;
            let event = match result {
                Ok(handle) => StorageEvent::ProjectSaved { handle },
                Err(error) => StorageEvent::Error(error),
            };
            let _ = sender.send(event);
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn web_import_audio(&mut self) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        if let Ok(promise) = crate::web::storage::request_file() {
            wasm_bindgen_futures::spawn_local(async move {
                let event = match crate::web::storage::first_file_from_promise(promise).await {
                    Ok((name, bytes)) => StorageEvent::AudioPicked { name, bytes },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }
        match crate::web::storage::fallback_file_input(sender, "audio/*", FilePurpose::Audio) {
            Ok(()) => {}
            Err(error) => {
                self.web_storage_rx = None;
                self.notify_error(error);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_import_extension_zip(&mut self) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        if let Ok(promise) = crate::web::storage::request_file() {
            wasm_bindgen_futures::spawn_local(async move {
                let event = match crate::web::storage::first_file_from_promise(promise).await {
                    Ok((name, bytes)) => StorageEvent::ExtensionZipPicked { name, bytes },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }
        match crate::web::storage::fallback_file_input(
            sender,
            ".zip,application/zip",
            FilePurpose::ExtensionZip,
        ) {
            Ok(()) => {}
            Err(error) => {
                self.web_storage_rx = None;
                self.notify_error(error);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web_storage(&mut self) {
        let Some(receiver) = self.web_storage_rx.take() else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            match event {
                StorageEvent::ProjectLoaded { handle, files } => {
                    match Workspace::from_bundle_files(&files, handle.name.clone()) {
                        Ok(mut workspace) => {
                            workspace.bundle =
                                Some(PathBuf::from(format!("browser://{}", handle.name)));
                            self.workspace = workspace;
                            self.web_project = Some(handle.clone());
                            crate::web::storage::remember_last_project(&handle.name);
                            self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                            self.ui.state.selection.clear();
                            self.recovery_error = None;
                            self.notify_info(format!("Opened {}", handle.name));
                        }
                        Err(error) => self.notify_error(error),
                    }
                }
                StorageEvent::ProjectSaved { handle } => {
                    self.workspace.bundle =
                        Some(PathBuf::from(format!("browser://{}", handle.name)));
                    self.workspace.dirty = false;
                    crate::web::storage::remember_last_project(&handle.name);
                    self.web_project = Some(handle.clone());
                    self.notify_info(format!("Saved {}", handle.name));
                }
                StorageEvent::AudioPicked { name, bytes } => {
                    match self.workspace.import_audio_bytes(&name, bytes) {
                        Ok(asset_id) => {
                            let track_id = self
                                .ui
                                .selected_track(&self.workspace.doc)
                                .or_else(|| self.create_audio_track_for_import());
                            if let Some(track_id) = track_id {
                                let at = self.workspace.doc.transport.playhead;
                                if let Err(error) =
                                    self.workspace.insert_audio_clip_at(track_id, asset_id, at)
                                {
                                    self.notify_error(error);
                                }
                            }
                            self.notify_info(format!("Imported {name}"));
                        }
                        Err(error) => self.notify_error(error),
                    }
                }
                StorageEvent::ExtensionZipPicked { name, bytes } => {
                    match self.extensions.install_zip_bytes(bytes) {
                        Ok(()) => self.notify_info(format!("Installing {name}…")),
                        Err(error) => self.notify_error(error),
                    }
                }
                StorageEvent::Error(error) => self.notify_error(error),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn create_audio_track_for_import(&mut self) -> Option<notwig_core::TrackId> {
        let before: std::collections::BTreeSet<notwig_core::TrackId> =
            self.workspace.doc.tracks.keys().copied().collect();
        if let Err(error) = self.workspace.create_track(TrackKind::Audio) {
            self.notify_error(error);
            return None;
        }
        self.workspace
            .doc
            .tracks
            .keys()
            .find(|id| !before.contains(id))
            .copied()
    }

    fn sync_extensions(&mut self) {
        self.workspace
            .set_extension_devices(self.extensions.specs(), self.extensions.factory());
        self.ui.set_device_panel_host(self.extensions.panel_host());
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
            Err(err) => {
                self.workspace.error = Some(err.clone());
                self.workspace.status = Some(err.clone());
                self.notify_error(format!("Could not start the audio engine: {err}"));
            }
        }
    }

    fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(AudioService::snapshot)
            .unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_audio_imports(&mut self) {
        while let Ok((asset_id, result)) = self.audio_import_rx.try_recv() {
            match result {
                Ok(imported) => {
                    if let Err(err) = self.workspace.complete_audio_import(asset_id, imported) {
                        self.notify_error(err);
                    }
                }
                Err(err) => self.notify_error(format!("Audio import failed: {err}")),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_audio_analysis(&mut self) {
        while let Ok((path, duration)) = self.audio_analysis_rx.try_recv() {
            self.audio_analysis_pending.remove(&path);
            self.audio_durations.insert(path, duration);
        }
    }

    fn advance_render(&mut self) {
        let Some(mut job) = self.render_job.take() else {
            self.ui.render_progress = None;
            self.render_playhead = None;
            return;
        };
        let total = job.session.total_frames();
        let mut error = None;
        let mut budget = 8;
        while budget > 0 && job.output.len() / 2 < total {
            let frames = (total - job.output.len() / 2).min(4096);
            match job.session.render_block(frames) {
                Ok(block) => job.output.extend(block),
                Err(err) => {
                    error = Some(err);
                    break;
                }
            }
            budget -= 1;
        }
        if let Some(err) = error {
            self.notify_error(err);
            self.ui.render_progress = None;
            self.render_playhead = None;
            return;
        }
        let done = job.output.len() / 2;
        let progress = (done as f32 / total.max(1) as f32).min(1.0);
        self.ui.render_progress = Some(progress);
        self.render_playhead = Some(
            self.workspace
                .doc
                .tempo_map()
                .samples_to_mus(job.session.start_sample.saturating_add(done as u64)),
        );
        if done >= total {
            job.output.truncate(total * 2);
            #[cfg(not(target_arch = "wasm32"))]
            {
                match notwig_project::write_audio_export(
                    &job.path,
                    job.session.sample_rate,
                    2,
                    &job.output,
                    job.options,
                ) {
                    Ok(()) => self.notify_info(format!(
                        "Rendered {} frames to {}",
                        total,
                        job.path.display()
                    )),
                    Err(err) => self.notify_error(err.to_string()),
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                match notwig_project::write_wav_f32_bytes(job.session.sample_rate, 2, &job.output) {
                    Ok(bytes) => {
                        let name = job
                            .path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("mixdown.wav");
                        match crate::web::storage::download_bytes(name, "audio/wav", &bytes) {
                            Ok(()) => self.notify_info(format!("Rendered {total} frames.")),
                            Err(error) => self.notify_error(error),
                        }
                    }
                    Err(error) => self.notify_error(error.to_string()),
                }
            }
            self.ui.render_progress = None;
            self.render_playhead = None;
        } else {
            self.render_job = Some(job);
        }
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
        );
        RuntimeView {
            playing: self.render_playhead.is_some() || self.workspace.doc.transport.playing,
            playhead,
            play_start: self
                .workspace
                .play_start
                .unwrap_or(self.workspace.doc.transport.playhead),
            waveforms: self.workspace.peaks.clone(),
            audio_durations: self.audio_durations.clone(),
            audio_analysis_pending: self.audio_analysis_pending.clone(),
            meters,
            xruns: snapshot.xruns,
            cpu_load: snapshot.cpu_load,
            status,
            engine_crashed: snapshot.crashed,
            plugins: self.workspace.plugins.clone(),
            bundle_path: self.workspace.bundle.clone(),
            dirty: self.workspace.dirty,
            saving: false,
            audio_settings,
            dsp: snapshot.dsp.clone(),
            clipboard: self.workspace.clipboard().clone(),
            extensions: self.extensions.infos(),
            extensions_error: self.extensions.error().map(str::to_owned),
            extension_indexes: self.extensions.indexes(),
            extension_index: self.extensions.index_entries(),
            extension_index_errors: self.extensions.index_errors(),
            extension_index_loading: self.extensions.index_loading(),
            extension_index_loaded: self.extensions.index_loaded(),
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
        if !(snapshot.running && self.workspace.doc.transport.playing) {
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

    fn current_playhead(&self, snapshot: &crate::audio::AudioSnapshot) -> TimeMus {
        if snapshot.running {
            self.interpolated_playhead(snapshot)
        } else if self.workspace.doc.transport.playing {
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

    #[cfg(not(target_arch = "wasm32"))]
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
                if let Some(state) = self.workspace.plugin_state(device_id) {
                    let _ = window.load_state(state);
                }
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

    #[cfg(target_arch = "wasm32")]
    fn open_plugin_window(&mut self, _device_id: DeviceId) {
        self.notify_error("Native plugin editors are not available in the browser.".into());
    }

    #[cfg(target_arch = "wasm32")]
    fn perform_drop(&mut self, _path: PathBuf, _target: DropAudioTarget, _at: TimeMus) {
        self.notify_info("Drop audio from the browser file picker instead.".into());
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
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(value) = raw_value
                    && let Some((_, window)) = self
                        .plugin_editors
                        .iter_mut()
                        .find(|(id, _)| *id == device_id)
                {
                    window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
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
            UiIntent::NewProject => match Workspace::new_empty() {
                Ok(workspace) => {
                    self.workspace.end_session();
                    self.workspace = workspace;
                    self.recovery_error = None;
                    self.fallback_playhead = 0.0;
                    self.restart_audio();
                    self.notify_info("New project created.".into());
                }
                Err(err) => self.notify_error(err),
            },
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
            #[cfg(target_arch = "wasm32")]
            UiIntent::Save => {
                self.sync_playhead();
                self.web_save_project(false);
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::SaveAs => self.handle_save_as(),
            #[cfg(target_arch = "wasm32")]
            UiIntent::SaveAs => self.web_save_project(true),
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::RenderMixdownRange {
                start,
                end,
                options,
            } => {
                if self.render_job.is_some() {
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
                if self.render_job.is_some() {
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
                self.restart_audio();
                if self.audio.is_some() {
                    self.notify_info("Audio engine restarted.".into());
                }
            }
            UiIntent::CancelRender => {
                if self.render_job.take().is_some() {
                    self.ui.render_progress = None;
                    self.render_playhead = None;
                    self.notify_info("Render cancelled.".into());
                }
            }
            UiIntent::SetExtensionEnabled { id, enabled } => {
                self.extensions.set_enabled(&id, enabled);
                self.sync_extensions();
            }
            UiIntent::SetExtensionTrust { id, trust } => {
                self.extensions.set_trust(&id, trust);
                self.sync_extensions();
            }
            UiIntent::ReloadExtensions => {
                self.extensions.reload();
                self.sync_extensions();
                if let Some(error) = self.extensions.error() {
                    self.notify_error(format!("Extension load failed: {error}"));
                } else {
                    self.notify_info("Extensions reloaded.".into());
                }
            }
            UiIntent::SetExtensionIndexes { indexes } => {
                self.extensions.set_indexes(indexes);
            }
            UiIntent::RefreshExtensionIndex => {
                self.extensions.refresh_index();
            }
            UiIntent::InstallExtensionFromIndex { id } => {
                if let Err(error) = self.extensions.install_from_index(&id) {
                    self.notify_error(error);
                }
            }
            UiIntent::InstallExtensionZip { path } => {
                if let Err(error) = self.extensions.install_zip(path) {
                    self.notify_error(error);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiIntent::ImportExtensionZip => {
                self.spawn_dialog(DialogAction::ImportExtensionZip, || {
                    pick_file("Import extension ZIP", "Notwig extension", &["zip"])
                });
            }
            #[cfg(target_arch = "wasm32")]
            UiIntent::ImportExtensionZip => self.web_import_extension_zip(),
            UiIntent::UninstallExtension { id } => match self.extensions.uninstall(&id) {
                Ok(()) => {
                    self.sync_extensions();
                    self.notify_info(format!("Uninstalled {id}."));
                }
                Err(error) => self.notify_error(error),
            },
            UiIntent::ClearNotifications => self.ui.state.notifications.clear(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_save_as(&mut self) {
        self.sync_playhead();
        let file_name = format!("{}.notwig", self.workspace.doc.meta.name);
        self.spawn_dialog(DialogAction::SaveAs, move || {
            save_file("Save Notwig project bundle", &file_name, "", &[])
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(not(target_arch = "wasm32"))]
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
            DialogAction::Render {
                start,
                end,
                options,
            } => {
                let Some(mut path) = path else {
                    return;
                };
                let export = export_options(options);
                if options.stems {
                    match self.workspace.render_stems_to(&path, start, end, export) {
                        Ok(stems) => self.notify_info(format!(
                            "Rendered {} stems to {}",
                            stems.len(),
                            path.display()
                        )),
                        Err(err) => self.notify_error(err),
                    }
                } else {
                    if path.extension().is_none() {
                        path.set_extension(options.format.extension());
                    }
                    match self.workspace.begin_render(start, end) {
                        Ok(session) => {
                            self.ui.render_progress = Some(0.0);
                            let capacity = session.total_frames().saturating_mul(2);
                            self.render_job = Some(RenderJob {
                                session,
                                output: Vec::with_capacity(capacity),
                                path,
                                options: export,
                            });
                        }
                        Err(err) => self.notify_error(err),
                    }
                }
            }
            DialogAction::ImportAudio => {
                let Some(path) = path else {
                    return;
                };
                let result = self.workspace.import_audio(&path);
                let _ = self.report(result);
            }
            DialogAction::ImportExtensionZip => {
                let Some(path) = path else {
                    return;
                };
                if let Err(error) = self.extensions.install_zip(path) {
                    self.notify_error(error);
                }
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

    #[cfg(not(target_arch = "wasm32"))]
    fn perform_drop(&mut self, path: PathBuf, target: DropAudioTarget, at: TimeMus) {
        if let DropAudioTarget::ClipEditor { clip_id } = target {
            match self.workspace.stage_audio_slice_import(&path, clip_id, at) {
                Ok((asset_id, target_dir)) => {
                    self.queue_audio_import(asset_id, path, target_dir);
                }
                Err(err) => self.notify_error(err),
            }
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
            match self.workspace.stage_audio_clip_import(&path, track_id, at) {
                Ok((asset_id, target_dir)) => {
                    self.queue_audio_import(asset_id, path, target_dir);
                }
                Err(err) => self.notify_error(err),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn queue_audio_import(&mut self, asset_id: AssetId, source: PathBuf, target: PathBuf) {
        let project_rate = self.workspace.doc.meta.sample_rate.value();
        let _ = self.audio_import_tx.send(AudioImportRequest {
            asset_id,
            source,
            target,
            project_rate,
        });
        self.notify_info("Analyzing audio…".into());
    }
}

impl Drop for NotwigApp {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        for (device_id, window) in &mut self.plugin_editors {
            if let Ok(state) = window.save_state() {
                self.workspace.store_plugin_state(*device_id, state);
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
        self.advance_render();
        #[cfg(target_arch = "wasm32")]
        self.poll_web_storage();
        self.workspace.poll_plugin_catalog();
        for event in self.extensions.poll() {
            match event {
                crate::extensions::ExtensionEvent::Installed { name, version } => {
                    self.sync_extensions();
                    self.notify_info(format!("Installed {name} {version}"));
                }
                crate::extensions::ExtensionEvent::Error(error) => self.notify_error(error),
            }
        }
        if self.extensions.index_loading() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut editor_changes = Vec::new();
            let mut closed_states = Vec::new();
            self.plugin_editors.retain_mut(|(device_id, window)| {
                window.pump();
                if !window.is_open() {
                    if let Ok(state) = window.save_state() {
                        closed_states.push((*device_id, state));
                    }
                    return false;
                }
                for (param_id, value) in window.poll_changes() {
                    editor_changes.push((*device_id, param_id, value));
                }
                true
            });
            for (device_id, state) in closed_states {
                self.workspace.set_plugin_state(device_id, state);
            }
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
        }
        let dialog_open = self.dialog_rx.is_some();
        let rendering = self.render_job.is_some();
        let dirty = if self.workspace.dirty { "*" } else { "" };
        let title = format!("{}{} — Notwig", self.workspace.doc.meta.name, dirty);
        #[cfg(not(target_arch = "wasm32"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(&title);
        }
        let snapshot = self.audio_snapshot();
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
        let runtime = self.runtime_view(&snapshot);
        #[cfg(not(target_arch = "wasm32"))]
        self.ui.set_platform_pointer(platform_pointer(ctx));
        #[cfg(target_arch = "wasm32")]
        self.ui
            .set_platform_pointer(ctx.input(|input| input.pointer.latest_pos()));
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

#[cfg(target_arch = "wasm32")]
fn sanitized_bundle_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn f64_to_time(value: f64) -> TimeMus {
    TimeMus::new((value.max(0.0) * 1_000_000.0).round() as i64, 1_000_000)
}

#[allow(dead_code)]
fn _keep(_: &ProjectDoc, _: Rational, _: DeviceKind, _: TrackKind) {}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn audio_pick_dialog(title: &'static str) -> impl FnOnce() -> Option<PathBuf> + Send + 'static {
    move || {
        pick_file(
            title,
            "Audio",
            &[
                "wav", "wave", "aif", "aiff", "aifc", "mp3", "flac", "ogg", "oga",
            ],
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_file(title: &str, filter_name: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(filter_name, extensions)
        .set_title(title)
        .pick_file()
}

#[cfg(not(target_arch = "wasm32"))]
fn save_file(
    title: &str,
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_file_name(file_name)
        .set_title(title);
    if !filter_name.is_empty() {
        dialog = dialog.add_filter(filter_name, extensions);
    }
    dialog.save_file()
}

fn export_options(options: notwig_ui::ExportOptions) -> notwig_project::ExportOptions {
    notwig_project::ExportOptions {
        format: match options.format {
            notwig_ui::ExportFormat::Wav => notwig_project::ExportFormat::Wav,
            notwig_ui::ExportFormat::Aiff => notwig_project::ExportFormat::Aiff,
            notwig_ui::ExportFormat::Flac => notwig_project::ExportFormat::Flac,
            notwig_ui::ExportFormat::Mp3 => notwig_project::ExportFormat::Mp3,
            notwig_ui::ExportFormat::Ogg => notwig_project::ExportFormat::Ogg,
        },
        bit_depth: match options.bit_depth {
            notwig_ui::ExportBitDepth::Int16 => notwig_project::ExportBitDepth::Int16,
            notwig_ui::ExportBitDepth::Int24 => notwig_project::ExportBitDepth::Int24,
            notwig_ui::ExportBitDepth::Float32 => notwig_project::ExportBitDepth::Float32,
        },
        dither: options.dither,
        normalize: options.normalize,
    }
}
