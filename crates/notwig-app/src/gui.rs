use crate::clock::{Duration, Instant};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use crate::audio::AudioService;
#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::{EngineClient, EngineEvent};
use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use mouse_position::mouse_position::Mouse;
use notwig_core::protocol::CommandPayload;
use notwig_core::{
    AssetId, AudioAssetBuffer, AudioInputRef, DeviceId, DeviceKind, ProjectDoc, Rational, TimeMus,
    TrackId, TrackKind,
};
#[cfg(target_arch = "wasm32")]
use notwig_ui::ProjectPickerAction;
use notwig_ui::{
    DropAudioTarget, NotificationKind, NotwigUi, RecentProjectView, RuntimeView, UiIntent,
};

#[cfg(target_arch = "wasm32")]
use crate::web::storage::{FilePurpose, ProjectHandle, StorageEvent};
use crate::workspace::{RenderSession, Workspace};

#[cfg(not(target_arch = "wasm32"))]
pub fn run(initial: Option<PathBuf>) -> Result<(), String> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../brand/notwig-app-icon.png"))
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

#[cfg(target_arch = "wasm32")]
struct WebProjectPicker {
    epoch: u64,
    action: ProjectPickerAction,
    projects: Vec<String>,
    loading: bool,
    error: Option<String>,
    suggested_name: String,
}

struct RenderJob {
    session: RenderSession,
    output: Vec<f32>,
    path: PathBuf,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    options: notwig_project::ExportOptions,
}

#[cfg(not(target_arch = "wasm32"))]
struct RenderEncodeResult {
    total_frames: usize,
    path: PathBuf,
    result: Result<(), String>,
}

#[cfg(not(target_arch = "wasm32"))]
struct StemsRenderResult {
    directory: PathBuf,
    result: Result<Vec<(PathBuf, usize)>, String>,
}

const RENDER_ENCODING_NOTIFICATION_TAG: &str = "render-encoding";

#[cfg(not(target_arch = "wasm32"))]
fn audio_import_notification_tag(asset_id: AssetId) -> String {
    format!("audio-import:{asset_id}")
}

const AUDIO_DEVICE_SCAN_INTERVAL: Duration = Duration::from_millis(1000);
const AUDIO_RESTART_INITIAL_DELAY: Duration = Duration::from_millis(500);
const AUDIO_RESTART_MAX_DELAY: Duration = Duration::from_secs(5);

fn available_audio_options() -> Vec<notwig_ui::AudioDriverView> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        EngineClient::available_options()
    }
    #[cfg(target_arch = "wasm32")]
    {
        AudioService::available_options()
    }
}

struct LiveRecordingTake {
    samples: Vec<f32>,
    waveform: Arc<notwig_ui::AudioWaveform>,
    waveform_dirty: bool,
    last_waveform_build: Instant,
}

struct ActiveRecording {
    started_at: TimeMus,
    sample_rate: u32,
    takes: std::collections::BTreeMap<TrackId, LiveRecordingTake>,
    end: Option<TimeMus>,
    compensation_samples: usize,
    #[cfg(target_arch = "wasm32")]
    session: Option<crate::recording::RecordingSession>,
}

fn new_live_take(sample_rate: u32) -> LiveRecordingTake {
    LiveRecordingTake {
        samples: Vec::new(),
        waveform: Arc::new(notwig_ui::AudioWaveform::new(Arc::new(AudioAssetBuffer {
            sample_rate,
            channels: 2,
            samples: Vec::new(),
        }))),
        waveform_dirty: false,
        last_waveform_build: Instant::now(),
    }
}

fn append_take_samples(active: &mut ActiveRecording, track_id: TrackId, samples: &[f32]) {
    let sample_rate = active.sample_rate;
    let take = active
        .takes
        .entry(track_id)
        .or_insert_with(|| new_live_take(sample_rate));
    take.samples.extend_from_slice(samples);
    take.waveform_dirty = true;
}

fn append_captures(active: &mut ActiveRecording, captures: Vec<(TrackId, Vec<f32>)>) {
    let skip = active.compensation_samples;
    let mut skipped = 0;
    for (track_id, samples) in captures {
        let skip_samples = skip.min(samples.len());
        append_take_samples(active, track_id, &samples[skip_samples..]);
        skipped = skipped.max(skip_samples);
    }
    active.compensation_samples = active.compensation_samples.saturating_sub(skipped);
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
                ..crate::audio::AudioSettings::default()
            },
            audio_options: available_audio_options(),
            recording: None,
            fallback_playhead,
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
        AudioService::prepare_input_options();
        self.restart_audio();
    }

    #[cfg(target_arch = "wasm32")]
    fn web_open_project(&mut self) {
        self.web_show_picker(ProjectPickerAction::Open);
    }

    #[cfg(target_arch = "wasm32")]
    fn web_open_project_folder(&mut self) {
        self.web_show_picker(ProjectPickerAction::Open);
    }

    #[cfg(target_arch = "wasm32")]
    fn web_show_picker(&mut self, action: ProjectPickerAction) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let suggested_name = self.web_project.as_ref().map_or_else(
            || web_project_name(&self.workspace.doc.meta.name),
            |handle| handle.name.clone(),
        );
        self.web_picker_epoch = self.web_picker_epoch.wrapping_add(1);
        let epoch = self.web_picker_epoch;
        self.web_picker = Some(WebProjectPicker {
            epoch,
            action,
            projects: Vec::new(),
            loading: true,
            error: None,
            suggested_name,
        });
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        wasm_bindgen_futures::spawn_local(async move {
            let result = crate::web::storage::list_projects().await;
            let _ = sender.send(StorageEvent::ProjectsListed { epoch, result });
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn web_open_project_named(&mut self, name: String) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        self.web_picker = None;
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let handle = crate::web::storage::open_project(&name).await?;
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
        if !choose_new && let Some(handle) = self.web_project.clone() {
            let files = match self.workspace.bundle_files() {
                Ok(files) => files,
                Err(error) => {
                    self.notify_error(error);
                    return;
                }
            };
            let (sender, receiver) = mpsc::channel();
            self.web_storage_rx = Some(receiver);
            wasm_bindgen_futures::spawn_local(async move {
                let event = match crate::web::storage::write_bundle(&handle, &files).await {
                    Ok(()) => StorageEvent::ProjectSaved { handle },
                    Err(error) => StorageEvent::Error(error),
                };
                let _ = sender.send(event);
            });
            return;
        }
        self.web_show_picker(ProjectPickerAction::SaveAs);
    }

    #[cfg(target_arch = "wasm32")]
    fn web_save_project_named(&mut self, name: String) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let name = web_project_name(&name);
        self.sync_playhead();
        let files = match self.workspace.bundle_files() {
            Ok(files) => files,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        self.web_picker = None;
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                crate::web::storage::delete_project(&name).await?;
                let handle = crate::web::storage::create_project(&name).await?;
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
    fn web_download_project(&mut self) {
        if self.web_storage_rx.is_some() {
            self.notify_info("A browser storage operation is already running.".into());
            return;
        }
        let Some(handle) = self.web_project.clone() else {
            self.notify_error("Save the project before downloading it.".into());
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.web_storage_rx = Some(receiver);
        wasm_bindgen_futures::spawn_local(async move {
            let name = handle.name.clone();
            let result = async {
                let files = crate::web::storage::read_project_files(&handle).await?;
                let bytes =
                    notwig_project::bundle_zip_bytes(&files).map_err(|error| error.to_string())?;
                crate::web::storage::download_bytes(
                    &format!("{name}.notwig.zip"),
                    "application/zip",
                    &bytes,
                )
            }
            .await;
            let _ = sender.send(StorageEvent::ProjectDownloaded { name, result });
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
        let Some(receiver) = self.web_storage_rx.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        let mut disconnected = false;
        loop {
            match receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.web_storage_rx = None;
        }
        for event in events {
            self.handle_web_storage_event(event);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_web_storage_event(&mut self, event: StorageEvent) {
        match event {
            StorageEvent::ProjectLoaded { handle, files } => {
                match Workspace::from_bundle_files(&files, handle.name.clone()) {
                    Ok(mut workspace) => {
                        workspace.bundle =
                            Some(PathBuf::from(format!("browser://{}", handle.name)));
                        self.workspace = workspace;
                        self.web_project = Some(handle.clone());
                        self.web_picker = None;
                        crate::web::storage::remember_recent_project(&handle.name);
                        self.refresh_recent_projects();
                        self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                        self.ui.state.selection.clear();
                        self.recovery_error = None;
                        self.sync_extensions();
                        self.restart_audio();
                        self.notify_info(format!("Opened {}", handle.name));
                    }
                    Err(error) => self.notify_error(error),
                }
            }
            StorageEvent::ProjectSaved { handle } => {
                self.workspace.bundle = Some(PathBuf::from(format!("browser://{}", handle.name)));
                self.workspace.dirty = false;
                crate::web::storage::remember_recent_project(&handle.name);
                self.refresh_recent_projects();
                self.web_project = Some(handle.clone());
                self.notify_info(format!("Saved {}", handle.name));
            }
            StorageEvent::ProjectsListed { epoch, result } => {
                if let Some(picker) = self
                    .web_picker
                    .as_mut()
                    .filter(|picker| picker.epoch == epoch)
                {
                    picker.loading = false;
                    match result {
                        Ok(projects) => picker.projects = projects,
                        Err(error) => picker.error = Some(error),
                    }
                }
            }
            StorageEvent::ProjectDownloaded { name, result } => match result {
                Ok(()) => self.notify_info(format!("Downloaded {name}.notwig.zip")),
                Err(error) => self.notify_error(error),
            },
            StorageEvent::AudioPicked { name, bytes } => {
                self.web_storage_rx = None;
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
                self.web_storage_rx = None;
                match self.extensions.install_zip_bytes(bytes) {
                    Ok(()) => self.notify_info(format!("Installing {name}…")),
                    Err(error) => self.notify_error(error),
                }
            }
            StorageEvent::FileSelectionCancelled => self.web_storage_rx = None,
            StorageEvent::Error(error) => {
                self.web_storage_rx = None;
                self.notify_error(error);
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
        self.workspace
            .set_extension_engine_runtime(self.extensions.engine_runtime());
        self.ui.set_device_panel_host(self.extensions.panel_host());
    }

    fn refresh_recent_projects(&mut self) {
        self.recent_projects = load_recent_projects();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn restart_audio(&mut self) {
        self.audio = None;
        let rate = self.workspace.doc.meta.sample_rate.value();
        match EngineClient::start(rate, &self.audio_settings) {
            Ok(audio) => {
                self.workspace.set_engine_process(Some(audio.handle()));
                self.audio = Some(audio);
                self.workspace.engine_audio_restarted();
                self.sync_recording_inputs();
                self.engine_stopped = false;
                self.clear_audio_device_lost(true);
            }
            Err(err) => {
                self.workspace.set_engine_process(None);
                self.audio_start_failed(err);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
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
                self.sync_recording_inputs();
                self.engine_stopped = false;
                self.clear_audio_device_lost(true);
            }
            Err(err) => {
                self.audio_start_failed(err);
            }
        }
        self.audio_options = available_audio_options();
    }

    fn audio_start_failed(&mut self, err: String) {
        if self.audio_device_lost || !self.configured_output_available() {
            self.enter_audio_device_lost(format!(
                "Audio output \"{}\" is unavailable: {err}",
                self.configured_output_label()
            ));
            self.schedule_audio_restart_after_failure();
        } else {
            self.workspace.error = Some(err.clone());
            self.workspace.status = Some(err.clone());
            self.notify_error(format!("Could not start the audio engine: {err}"));
        }
    }

    fn configured_output_label(&self) -> String {
        if self.audio_settings.device.is_empty()
            || self.audio_settings.device == crate::audio::SYSTEM_OUT_DEVICE_ID
        {
            "System Out".to_string()
        } else {
            self.audio_settings.device.clone()
        }
    }

    fn uses_system_output(&self) -> bool {
        self.audio_settings.device.is_empty()
            || self.audio_settings.device == crate::audio::SYSTEM_OUT_DEVICE_ID
    }

    fn configured_output_available(&self) -> bool {
        crate::audio::resolve_output_device_name(
            &self.audio_settings.driver,
            &self.audio_settings.device,
        )
        .is_ok()
    }

    fn rescan_audio_devices(&mut self) {
        self.audio_options = available_audio_options();
    }

    fn enter_audio_device_lost(&mut self, message: String) {
        let was_lost = self.audio_device_lost;
        self.audio_device_lost = true;
        self.audio_device_lost_message = Some(message.clone());
        if !was_lost {
            self.next_audio_device_scan = Instant::now();
            self.audio_restart_backoff = AUDIO_RESTART_INITIAL_DELAY;
            self.audio_restart_at = Some(Instant::now() + AUDIO_RESTART_INITIAL_DELAY);
            if self.workspace.doc.transport.playing || self.recording.is_some() {
                self.handle_intent(UiIntent::Pause);
            }
            self.rescan_audio_devices();
            self.workspace.error = None;
            self.ui
                .state
                .notify(NotificationKind::Warning, message.clone());
        }
        self.workspace.status = Some(message);
    }

    fn clear_audio_device_lost(&mut self, notify: bool) {
        if !self.audio_device_lost {
            return;
        }
        self.audio_device_lost = false;
        self.audio_device_lost_message = None;
        self.audio_restart_at = None;
        self.audio_restart_backoff = AUDIO_RESTART_INITIAL_DELAY;
        if notify {
            self.notify_info("Audio output restored.".into());
        }
    }

    fn schedule_audio_restart_after_failure(&mut self) {
        self.audio_restart_at = Some(Instant::now() + self.audio_restart_backoff);
        self.audio_restart_backoff = (self.audio_restart_backoff * 2).min(AUDIO_RESTART_MAX_DELAY);
    }

    fn restart_audio_for_output_change(&mut self) {
        let was_lost = self.audio_device_lost;
        self.restart_audio();
        if self.audio.is_some() && !was_lost {
            self.notify_info("Audio output changed.".into());
        }
    }

    fn poll_audio_device_state(&mut self, snapshot: &crate::audio::AudioSnapshot) {
        if snapshot.device_lost {
            let reason = snapshot
                .device_lost_reason
                .as_deref()
                .map_or(String::new(), |reason| format!(": {reason}"));
            self.enter_audio_device_lost(format!(
                "Audio output \"{}\" is unavailable{reason}",
                snapshot.device_name
            ));
        } else if snapshot.device_changed && !self.audio_device_lost {
            self.rescan_audio_devices();
            self.restart_audio_for_output_change();
            return;
        }

        let now = Instant::now();
        if now < self.next_audio_device_scan {
            return;
        }
        self.next_audio_device_scan = now + AUDIO_DEVICE_SCAN_INTERVAL;
        self.rescan_audio_devices();

        if self.audio_device_lost {
            if self
                .audio_restart_at
                .is_none_or(|restart_at| now >= restart_at)
                && self.configured_output_available()
            {
                self.restart_audio_for_output_change();
            } else if !self.configured_output_available() {
                self.audio_restart_at = None;
            }
            return;
        }

        if snapshot.running
            && self.uses_system_output()
            && let Ok(default_name) = crate::audio::resolve_output_device_name(
                &self.audio_settings.driver,
                &self.audio_settings.device,
            )
            && !snapshot.device_name.is_empty()
            && default_name != snapshot.device_name
        {
            self.restart_audio_for_output_change();
        }
    }

    fn armed_input_requests(&self) -> Vec<(TrackId, AudioInputRef)> {
        self.workspace
            .doc
            .tracks
            .values()
            .filter(|track| track.kind == TrackKind::Audio && track.record_armed)
            .filter_map(|track| {
                track
                    .io
                    .audio_input
                    .clone()
                    .filter(|input| {
                        input.driver == self.audio_settings.driver
                            && input.device == self.audio_settings.device
                    })
                    .map(|input| (track.id, input))
            })
            .collect()
    }

    fn sync_recording_inputs(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let requests = self.armed_input_requests();
        if self.workspace.doc.transport.recording
            && self.workspace.doc.transport.playing
            && !requests.is_empty()
        {
            self.begin_recording_capture(None);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                return;
            };
            if let Err(error) = audio.handle().configure_inputs(requests) {
                self.notify_error(error);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(audio) = &self.audio else {
                return;
            };
            match audio.configure_inputs(&requests) {
                Ok(routes) => self.workspace.set_input_routes(routes),
                Err(error) => self.notify_error(error),
            }
        }
    }

    fn toggle_record(&mut self) {
        if self.workspace.doc.transport.recording {
            let result = self
                .workspace
                .apply(CommandPayload::SetRecording { recording: false });
            self.stop_recording_capture();
            let _ = self.report(result);
        } else {
            let result = self
                .workspace
                .apply(CommandPayload::SetRecording { recording: true });
            if self.report(result).is_some() && self.workspace.doc.transport.playing {
                self.begin_recording_capture(None);
            }
        }
    }

    fn begin_recording_capture(&mut self, started_at: Option<TimeMus>) {
        if self.recording.is_some() {
            return;
        }
        let requests = self.armed_input_requests();
        if requests.is_empty() {
            return;
        }
        let snapshot = self.audio_snapshot();
        let started_at = started_at.unwrap_or_else(|| self.current_playhead(&snapshot));
        let sample_rate = self.workspace.doc.meta.sample_rate.value();
        let block_size = if snapshot.block_size > 0 {
            snapshot.block_size
        } else {
            self.audio_settings.block_size
        };
        let compensation_samples = usize::try_from(block_size.max(1))
            .unwrap_or(1)
            .saturating_mul(2);
        let mut takes = std::collections::BTreeMap::new();
        for (track_id, _) in &requests {
            takes.insert(*track_id, new_live_take(sample_rate));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                self.notify_error("The audio engine is not running.".into());
                return;
            };
            if let Err(error) = audio.handle().configure_inputs(requests) {
                self.notify_error(error);
                return;
            }
            if let Err(error) = audio.handle().start_recording() {
                self.notify_error(error);
                return;
            }
            self.recording = Some(ActiveRecording {
                started_at,
                sample_rate,
                takes,
                end: None,
                compensation_samples,
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(audio) = &self.audio else {
                self.notify_error("The audio engine is not running.".into());
                return;
            };
            let routes = match audio.configure_inputs(&requests) {
                Ok(routes) => {
                    self.workspace.set_input_routes(routes.clone());
                    routes
                }
                Err(error) => {
                    self.notify_error(error);
                    return;
                }
            };
            let session = audio.begin_recording(routes, sample_rate);
            self.recording = Some(ActiveRecording {
                started_at,
                sample_rate,
                takes,
                end: None,
                compensation_samples,
                session: Some(session),
            });
        }
    }

    fn stop_recording_capture(&mut self) {
        let snapshot = self.audio_snapshot();
        let end = self.current_playhead(&snapshot);
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        active.end = Some(end);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(audio) = &self.audio else {
                let active = self.recording.take().expect("recording was just checked");
                self.finalize_live_recording(active, end);
                return;
            };
            if let Err(error) = audio.handle().stop_recording() {
                let active = self.recording.take().expect("recording was just checked");
                self.finalize_live_recording(active, end);
                self.notify_error(error);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(mut active) = self.recording.take() else {
                return;
            };
            if let Some(session) = active.session.take() {
                match session.finish() {
                    Ok(captures) => {
                        let captures = captures
                            .into_iter()
                            .map(|(track_id, buffer)| (track_id, buffer.samples))
                            .collect();
                        append_captures(&mut active, captures);
                    }
                    Err(error) => self.notify_error(error),
                }
            }
            self.finalize_live_recording(active, end);
        }
    }

    fn append_recording_chunk(&mut self, sample_rate: u32, captures: Vec<(TrackId, Vec<f32>)>) {
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        active.sample_rate = sample_rate;
        append_captures(active, captures);
    }

    fn refresh_recording_waveforms(&mut self) {
        let Some(active) = self.recording.as_mut() else {
            return;
        };
        let now = Instant::now();
        for take in active.takes.values_mut() {
            let first_build = take.waveform.frame_count() == 0;
            if !take.waveform_dirty
                || (!first_build
                    && now.duration_since(take.last_waveform_build) < Duration::from_millis(100))
            {
                continue;
            }
            take.last_waveform_build = now;
            take.waveform_dirty = false;
            take.waveform = Arc::new(notwig_ui::AudioWaveform::new(Arc::new(AudioAssetBuffer {
                sample_rate: active.sample_rate,
                channels: 2,
                samples: take.samples.clone(),
            })));
        }
    }

    fn recording_take_views(&self, playhead: TimeMus) -> Vec<notwig_ui::RecordingTakeView> {
        let Some(active) = &self.recording else {
            return Vec::new();
        };
        let map = self.workspace.doc.tempo_map();
        active
            .takes
            .iter()
            .filter_map(|(track_id, take)| {
                let sample_end =
                    active.started_at + map.samples_to_mus(take.waveform.frame_count());
                let end = if playhead > active.started_at {
                    playhead.max(sample_end)
                } else {
                    sample_end
                };
                if end <= active.started_at {
                    return None;
                }
                Some(notwig_ui::RecordingTakeView {
                    track_id: *track_id,
                    start: active.started_at,
                    end,
                    waveform: Arc::clone(&take.waveform),
                })
            })
            .collect()
    }

    fn finalize_live_recording(&mut self, active: ActiveRecording, end: TimeMus) {
        self.recording = None;
        if end <= active.started_at {
            return;
        }
        let frames = usize::try_from(
            self.workspace
                .doc
                .tempo_map()
                .mus_to_samples(end - active.started_at),
        )
        .unwrap_or(0);
        let target_samples = frames.saturating_mul(2);
        let mut last_clip = None;
        for (track_id, mut take) in active.takes {
            if take.samples.is_empty() || target_samples == 0 {
                continue;
            }
            take.samples.resize(target_samples, 0.0);
            take.samples.truncate(target_samples);
            let buffer = AudioAssetBuffer {
                sample_rate: active.sample_rate,
                channels: 2,
                samples: take.samples,
            };
            match self
                .workspace
                .finish_recording(track_id, active.started_at, buffer)
            {
                Ok(clip_id) => last_clip = Some((track_id, clip_id)),
                Err(error) => self.notify_error(error),
            }
        }
        if let Some((track_id, clip_id)) = last_clip {
            self.ui.state.selection.select_track(track_id, false);
            self.ui
                .state
                .selection
                .select_clip(clip_id, track_id, false);
        }
        self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web_recording(&mut self) {
        let Some(active) = &self.recording else {
            return;
        };
        let Some(session) = &active.session else {
            return;
        };
        session.poll();
        let sample_rate = active.sample_rate;
        let captures = session.take_captured();
        if captures.is_empty() {
            return;
        }
        self.append_recording_chunk(sample_rate, captures.into_iter().collect());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(EngineClient::snapshot)
            .unwrap_or_default()
    }

    #[cfg(target_arch = "wasm32")]
    fn audio_snapshot(&self) -> crate::audio::AudioSnapshot {
        self.audio
            .as_ref()
            .map(AudioService::snapshot)
            .unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_audio_imports(&mut self) {
        while let Ok((asset_id, result)) = self.audio_import_rx.try_recv() {
            self.ui
                .state
                .remove_notification_by_tag(&audio_import_notification_tag(asset_id));
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
                let path = job.path;
                let sample_rate = job.session.sample_rate;
                let options = job.options;
                let output = job.output;
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result =
                        notwig_project::write_audio_export(&path, sample_rate, 2, &output, options)
                            .map_err(|err| err.to_string());
                    let _ = tx.send(RenderEncodeResult {
                        total_frames: total,
                        path,
                        result,
                    });
                });
                self.render_encode_rx = Some(rx);
                self.ui.state.notify_persistent_indeterminate(
                    NotificationKind::Info,
                    "Encoding audio…",
                    RENDER_ENCODING_NOTIFICATION_TAG,
                );
            }
            #[cfg(target_arch = "wasm32")]
            {
                self.pending_web_encode = Some(job);
                self.ui.state.notify_persistent_indeterminate(
                    NotificationKind::Info,
                    "Encoding audio…",
                    RENDER_ENCODING_NOTIFICATION_TAG,
                );
            }
            self.ui.render_progress = None;
            self.render_playhead = None;
        } else {
            self.render_job = Some(job);
        }
    }

    fn render_busy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.render_job.is_some()
                || self.render_encode_rx.is_some()
                || self.stems_render_rx.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.render_job.is_some() || self.pending_web_encode.is_some()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_render_encode(&mut self) {
        let received = match self.render_encode_rx.as_ref() {
            Some(rx) => rx.try_recv(),
            None => return,
        };
        match received {
            Ok(encoded) => {
                self.render_encode_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                match encoded.result {
                    Ok(()) => self.notify_info(format!(
                        "Rendered {} frames to {}",
                        encoded.total_frames,
                        encoded.path.display()
                    )),
                    Err(err) => self.notify_error(err),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.render_encode_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                self.notify_error("Audio encoding stopped unexpectedly.".into());
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_stems_render(&mut self) {
        let received = match self.stems_render_rx.as_ref() {
            Some(rx) => rx.try_recv(),
            None => return,
        };
        match received {
            Ok(completed) => {
                self.stems_render_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                match completed.result {
                    Ok(stems) => self.notify_info(format!(
                        "Rendered {} stems to {}",
                        stems.len(),
                        completed.directory.display()
                    )),
                    Err(err) => self.notify_error(err),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.stems_render_rx = None;
                self.ui
                    .state
                    .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
                self.notify_error("Stem rendering stopped unexpectedly.".into());
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web_encode(&mut self) {
        let Some(job) = self.pending_web_encode.take() else {
            return;
        };
        let total = job.session.total_frames();
        self.ui
            .state
            .remove_notification_by_tag(RENDER_ENCODING_NOTIFICATION_TAG);
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
        let _ = self.workspace.sync_playhead_for_save(playhead);
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
        #[cfg(not(target_arch = "wasm32"))]
        if !self.workspace.recovery_candidates.is_empty() {
            self.show_recovery_candidates_dialog(ctx);
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
    fn show_recovery_candidates_dialog(&mut self, ctx: &egui::Context) {
        let candidates = self.workspace.recovery_candidates.clone();
        if self.recovery_selection.as_ref().is_none_or(|selected| {
            !candidates
                .iter()
                .any(|candidate| &candidate.bundle == selected)
        }) {
            self.recovery_selection = candidates.first().map(|candidate| candidate.bundle.clone());
        }
        let mut select = None;
        let mut recover = false;
        let mut discard = false;
        egui::Modal::new(egui::Id::new("recovery-modal")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Recover unsaved projects?");
            ui.label(
                "One or more projects did not close cleanly. Select a project to recover or discard its autosaves.",
            );
            if let Some(error) = &self.recovery_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.separator();
            for candidate in &candidates {
                let selected = self.recovery_selection.as_ref() == Some(&candidate.bundle);
                let label = format!(
                    "{} ({} autosave(s))\n{}",
                    candidate.project_name,
                    candidate.autosave_count,
                    candidate.bundle.display()
                );
                if ui.selectable_label(selected, label).clicked() {
                    select = Some(candidate.bundle.clone());
                }
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
        if let Some(bundle) = select {
            self.recovery_selection = Some(bundle);
        }
        let Some(bundle) = self.recovery_selection.clone() else {
            return;
        };
        if recover {
            match self.workspace.recover_candidate(&bundle) {
                Ok(()) => {
                    self.recovery_error = None;
                    self.recovery_selection = None;
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
            match self.workspace.discard_candidate(&bundle) {
                Ok(()) => {
                    self.recovery_error = None;
                    self.recovery_selection = None;
                    self.fallback_playhead = self.workspace.doc.transport.playhead.as_f64();
                    self.ui.state.selection.clear();
                    self.workspace.begin_session();
                    self.restart_audio();
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
        let Some(descriptor) = self.workspace.plugin_descriptor(device_id) else {
            self.notify_error("Plugin is not installed or was not found.".into());
            return;
        };
        let Some(audio) = &self.audio else {
            self.notify_error("Audio engine is not running.".into());
            return;
        };
        let sample_rate = self.workspace.doc.meta.sample_rate.value();
        let state = self.workspace.plugin_state(device_id).cloned();
        let params = self
            .workspace
            .doc
            .devices
            .get(&device_id)
            .map(|device| {
                device
                    .param_values
                    .iter()
                    .filter_map(|(param_id, value)| value.as_f64().map(|value| (*param_id, value)))
                    .collect()
            })
            .unwrap_or_default();
        if let Err(err) = audio.open_editor(device_id, descriptor, sample_rate, state, params) {
            self.notify_error(format!("Could not open plugin window: {err}"));
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

    #[cfg(target_arch = "wasm32")]
    fn perform_drop_bytes(
        &mut self,
        name: String,
        bytes: Arc<[u8]>,
        target: DropAudioTarget,
        at: TimeMus,
    ) {
        let asset_id = match self.workspace.import_audio_bytes(&name, bytes.to_vec()) {
            Ok(asset_id) => asset_id,
            Err(error) => {
                self.notify_error(error);
                return;
            }
        };
        let result = match target {
            DropAudioTarget::ClipEditor { clip_id } => {
                self.workspace.add_audio_slice(clip_id, asset_id, at)
            }
            DropAudioTarget::Track(track_id) => {
                self.workspace.insert_audio_clip_at(track_id, asset_id, at)
            }
            DropAudioTarget::NewTrack { index } => {
                match self
                    .workspace
                    .create_track_at(TrackKind::Audio, Some(index))
                {
                    Ok(track_id) => self.workspace.insert_audio_clip_at(track_id, asset_id, at),
                    Err(error) => Err(error),
                }
            }
        };
        match result {
            Ok(()) => self.notify_info(format!("Imported {name}")),
            Err(error) => self.notify_error(error),
        }
    }

    fn handle_intent(&mut self, intent: UiIntent) {
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
                if self.recording.is_some() {
                    self.stop_recording_capture();
                }
                self.sync_playhead();
                let result = self.workspace.pause();
                let _ = self.report(result);
            }
            UiIntent::Stop => {
                if self.recording.is_some() {
                    self.stop_recording_capture();
                }
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
                    channels,
                    master_output,
                };
                self.workspace.audio_block_size = if block_size == 0 { 512 } else { block_size };
                self.workspace.request_engine_rebuild();
                self.audio_options = available_audio_options();
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
                    #[cfg(target_arch = "wasm32")]
                    {
                        self.web_project = None;
                        self.web_picker = None;
                        self.sync_extensions();
                    }
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
                self.extensions.set_enabled(&id, enabled);
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
            #[cfg(target_arch = "wasm32")]
            UiIntent::InstallExtensionZipBytes { name, bytes } => {
                match self.extensions.install_zip_bytes(bytes.to_vec()) {
                    Ok(()) => self.notify_info(format!("Installing {name}…")),
                    Err(error) => self.notify_error(error),
                }
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
                        self.refresh_recent_projects();
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
                        self.refresh_recent_projects();
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
                    let job = self.workspace.stems_render_job(&path, start, end, export);
                    let directory = path;
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let result = job.render();
                        let _ = tx.send(StemsRenderResult { directory, result });
                    });
                    self.stems_render_rx = Some(rx);
                    self.ui.state.notify_persistent_indeterminate(
                        NotificationKind::Info,
                        "Encoding audio…",
                        RENDER_ENCODING_NOTIFICATION_TAG,
                    );
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
        self.ui.state.notify_persistent_indeterminate(
            NotificationKind::Info,
            "Analyzing audio…",
            audio_import_notification_tag(asset_id),
        );
        self.workspace.status = Some("Analyzing audio…".into());
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
        for event in self.extensions.poll() {
            match event {
                crate::extensions::ExtensionEvent::Loaded => self.sync_extensions(),
                crate::extensions::ExtensionEvent::Installed { name, version } => {
                    self.sync_extensions();
                    self.notify_info(format!("Installed {name} {version}"));
                }
                crate::extensions::ExtensionEvent::Error(error) => self.notify_error(error),
            }
        }
        if self.extensions.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
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

#[cfg(target_arch = "wasm32")]
#[cfg(target_arch = "wasm32")]
fn web_project_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch == '\0' || ch.is_control() {
                '-'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|ch| ch == ' ' || ch == '.');
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_recent_projects() -> Vec<RecentProjectView> {
    crate::workspace::project_registry::registered_bundles()
        .into_iter()
        .rev()
        .filter_map(|path| {
            let label = path
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())?
                .to_owned();
            Some(RecentProjectView {
                label,
                key: path.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn load_recent_projects() -> Vec<RecentProjectView> {
    crate::web::storage::recent_project_names()
        .into_iter()
        .map(|name| RecentProjectView {
            label: name.clone(),
            key: name,
        })
        .collect()
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
