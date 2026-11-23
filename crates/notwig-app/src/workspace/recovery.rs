use super::Workspace;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

impl Workspace {
    #[cfg(target_arch = "wasm32")]
    pub fn begin_session(&mut self) {}

    #[cfg(target_arch = "wasm32")]
    pub fn end_session(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub fn begin_session(&mut self) {
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        if let Err(err) = notwig_project::create_session_lock(&bundle) {
            tracing::warn!(
                path = %bundle.display(),
                error = %err,
                "could not create session lock"
            );
            self.status = Some(format!("Could not create session lock: {err}"));
        }
        if let Err(err) = super::project_registry::register_bundle(&bundle) {
            tracing::warn!(
                path = %bundle.display(),
                error = %err,
                "could not register project bundle"
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn end_session(&mut self) {
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        if let Err(err) = notwig_project::remove_session_lock(&bundle) {
            tracing::warn!(
                path = %bundle.display(),
                error = %err,
                "could not remove session lock"
            );
            self.status = Some(format!("Could not remove session lock: {err}"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_startup_recovery_candidates(&mut self) {
        if self.bundle.is_some() {
            return;
        }
        let candidates = super::project_registry::recovery_candidates();
        if candidates.is_empty() {
            return;
        }
        self.status = Some(format!(
            "{} unsaved project(s) can be recovered.",
            candidates.len()
        ));
        self.recovery_candidates = candidates;
        self.recovery_offered = true;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn recover_candidate(&mut self, bundle: &Path) -> Result<(), String> {
        let mut workspace = Workspace::open(bundle)?;
        // On failure the candidate workspace is simply dropped. Its lock and
        // autosaves stay untouched so the recovery can be retried or discarded.
        workspace.recover_latest_autosave()?;
        if self.bundle.as_deref() != Some(bundle) {
            self.end_session();
        }
        *self = workspace;
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn discard_candidate(&mut self, bundle: &Path) -> Result<(), String> {
        let mut workspace = Workspace::open(bundle)?;
        // Discard is transactional: an error leaves the lock and snapshots
        // available for a later recovery attempt.
        workspace.discard_recovery()?;
        if self.bundle.as_deref() != Some(bundle) {
            self.end_session();
        }
        *self = workspace;
        Ok(())
    }

    pub fn recover_latest_autosave(&mut self) -> Result<(), String> {
        let bundle = self
            .bundle
            .clone()
            .ok_or_else(|| "project has no bundle path".to_string())?;
        let doc = notwig_project::load_latest_autosave(&bundle)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "no autosave snapshots are available".to_string())?;
        self.doc = doc;
        self.dirty = true;
        self.recovery_offered = false;
        self.play_start = None;
        self.history = crate::history::History::default();
        self.load_assets();
        // Autosave only stores the document, so its state refs can point at
        // blobs written by an earlier save. Reload them for the recovered doc.
        self.load_plugin_states();
        self.request_engine_rebuild();
        self.last_autosave = crate::clock::Instant::now();
        self.error = None;
        self.status = Some("Recovered latest autosave.".into());
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn discard_recovery(&mut self) -> Result<(), String> {
        let Some(bundle) = self.bundle.clone() else {
            self.recovery_offered = false;
            return Ok(());
        };
        let autosaves = notwig_project::list_autosaves(&bundle);
        if autosaves.is_empty() {
            notwig_project::remove_session_lock(&bundle).map_err(|err| err.to_string())?;
            self.recovery_offered = false;
            self.status = Some("Discarded recovery autosaves.".into());
            return Ok(());
        }

        // Move snapshots aside before removing the lock. If any step fails,
        // restore them so the candidate remains recoverable.
        let staging = bundle
            .join("autosave")
            .join(format!(".discard-{}", std::process::id()));
        match std::fs::remove_dir_all(&staging) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!("could not clear recovery staging: {err}"));
            }
        }
        std::fs::create_dir_all(&staging)
            .map_err(|err| format!("could not stage recovery discard: {err}"))?;
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(autosaves.len());
        for path in autosaves {
            let Some(file_name) = path.file_name() else {
                continue;
            };
            let staged = staging.join(file_name);
            if let Err(err) = std::fs::rename(&path, &staged) {
                restore_autosaves(&moved);
                let _ = std::fs::remove_dir_all(&staging);
                return Err(format!(
                    "could not stage autosave {}: {err}",
                    path.display()
                ));
            }
            moved.push((staged, path));
        }
        if let Err(err) = notwig_project::remove_session_lock(&bundle) {
            restore_autosaves(&moved);
            let _ = std::fs::remove_dir_all(&staging);
            return Err(err.to_string());
        }
        if let Err(err) = std::fs::remove_dir_all(&staging) {
            tracing::warn!(
                path = %staging.display(),
                error = %err,
                "could not delete staged recovery autosaves"
            );
        }
        self.recovery_offered = false;
        self.status = Some("Discarded recovery autosaves.".into());
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn discard_recovery(&mut self) -> Result<(), String> {
        self.recovery_offered = false;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn restore_autosaves(moved: &[(PathBuf, PathBuf)]) {
    for (staged, original) in moved.iter().rev() {
        if let Err(err) = std::fs::rename(staged, original) {
            tracing::warn!(
                from = %staged.display(),
                to = %original.display(),
                error = %err,
                "could not restore staged autosave"
            );
        }
    }
}
