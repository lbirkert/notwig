use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use notwig_core::TimeMus;
use notwig_ui::NotificationKind;

#[cfg(not(target_arch = "wasm32"))]
use super::platform::save_file;
#[cfg(not(target_arch = "wasm32"))]
use super::render::{
    RENDER_ENCODING_NOTIFICATION_TAG, RenderJob, StemsRenderResult, export_options,
};
use crate::workspace::Workspace;

use super::NotwigApp;

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub(super) enum DialogAction {
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

impl NotwigApp {
    pub(super) fn notify_error(&mut self, message: String) {
        self.workspace.error = Some(message.clone());
        self.workspace.status = Some(message.clone());
        self.ui.state.notify(NotificationKind::Error, message);
    }

    pub(super) fn report<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.notify_error(err);
                None
            }
        }
    }

    pub(super) fn notify_info(&mut self, message: String) {
        self.ui
            .state
            .notify(NotificationKind::Info, message.clone());
        self.workspace.status = Some(message);
    }

    pub(super) fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
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
    pub(super) fn handle_save_as(&mut self) {
        self.sync_playhead();
        let file_name = format!("{}.notwig", self.workspace.doc.meta.name);
        self.spawn_dialog(DialogAction::SaveAs, move || {
            save_file("Save Notwig project bundle", &file_name, "", &[])
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn spawn_dialog<F>(&mut self, action: DialogAction, open: F)
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
    pub(super) fn poll_dialog(&mut self) {
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
    pub(super) fn apply_dialog(&mut self, action: DialogAction, path: Option<PathBuf>) {
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
}
