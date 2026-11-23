use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, mpsc};

#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::EngineClient;
use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use mouse_position::mouse_position::Mouse;
use notwig_core::{TimeMus, TrackKind};
#[cfg(target_arch = "wasm32")]
use notwig_ui::ProjectPickerAction;
use notwig_ui::{DropAudioTarget, RecentProjectView};

#[cfg(target_arch = "wasm32")]
use crate::audio::AudioService;
#[cfg(target_arch = "wasm32")]
use crate::web::storage::{FilePurpose, ProjectHandle, StorageEvent};

use super::NotwigApp;

#[cfg(target_arch = "wasm32")]
pub(super) struct WebProjectPicker {
    pub(super) epoch: u64,
    pub(super) action: ProjectPickerAction,
    pub(super) projects: Vec<String>,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) suggested_name: String,
}

pub(super) fn available_audio_options() -> Vec<notwig_ui::AudioDriverView> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        EngineClient::available_options()
    }
    #[cfg(target_arch = "wasm32")]
    {
        AudioService::available_options()
    }
}

impl NotwigApp {
    pub(super) fn refresh_recent_projects(&mut self) {
        self.recent_projects = load_recent_projects();
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn web_open_project(&mut self) {
        self.web_show_picker(ProjectPickerAction::Open);
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn web_open_project_folder(&mut self) {
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
    pub(super) fn web_open_project_named(&mut self, name: String) {
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
    pub(super) fn web_save_project(&mut self, choose_new: bool) {
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
    pub(super) fn web_save_project_named(&mut self, name: String) {
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
            let result = crate::web::storage::save_as_project(&name, &files).await;
            let event = match result {
                Ok(handle) => StorageEvent::ProjectSaved { handle },
                Err(error) => StorageEvent::Error(error),
            };
            let _ = sender.send(event);
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn web_download_project(&mut self) {
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
    pub(super) fn web_import_audio(&mut self) {
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
    pub(super) fn web_import_extension_zip(&mut self) {
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
    pub(super) fn poll_web_storage(&mut self) {
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
                match crate::workspace::Workspace::from_bundle_files(&files, handle.name.clone()) {
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

    #[cfg(target_arch = "wasm32")]
    pub(super) fn perform_drop(&mut self, _path: PathBuf, _target: DropAudioTarget, _at: TimeMus) {
        self.notify_info("Drop audio from the browser file picker instead.".into());
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn perform_drop_bytes(
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

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn perform_drop(&mut self, path: PathBuf, target: DropAudioTarget, at: TimeMus) {
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
                    .create_track_at(TrackKind::Audio, Some(index))
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
}

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
pub(super) fn load_recent_projects() -> Vec<RecentProjectView> {
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
pub(super) fn load_recent_projects() -> Vec<RecentProjectView> {
    crate::web::storage::recent_project_names()
        .into_iter()
        .map(|name| RecentProjectView {
            label: name.clone(),
            key: name,
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn platform_pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
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
pub(super) fn audio_pick_dialog(
    title: &'static str,
) -> impl FnOnce() -> Option<PathBuf> + Send + 'static {
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
pub(super) fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn pick_file(title: &str, filter_name: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(filter_name, extensions)
        .set_title(title)
        .pick_file()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save_file(
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
