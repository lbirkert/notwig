use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use crate::clock::Instant;
#[cfg(not(target_arch = "wasm32"))]
use crate::clock::{SystemTime, UNIX_EPOCH};
#[cfg(not(target_arch = "wasm32"))]
use notwig_core::StateBlobRef;

#[cfg(not(target_arch = "wasm32"))]
use super::PLUGIN_STATE_ENCODING;
use super::Workspace;

impl Workspace {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&mut self, as_path: Option<&Path>) -> Result<PathBuf, String> {
        let path = match as_path {
            Some(path) => path.to_path_buf(),
            None => self
                .bundle
                .clone()
                .ok_or_else(|| "project has no bundle path; use Save As".to_string())?,
        };
        if !self.pending_audio_imports.is_empty() {
            return Err(
                "Audio import is still running; wait for it to finish before saving.".into(),
            );
        }
        let previous = self.bundle.clone();
        if !self.plugin_states.is_empty() {
            let state_dir = path.join("state");
            std::fs::create_dir_all(&state_dir).map_err(|err| err.to_string())?;
            for (device_id, state) in &self.plugin_states {
                let bytes = serde_json::to_vec(state).map_err(|err| err.to_string())?;
                std::fs::write(state_dir.join(format!("{device_id}.json")), bytes)
                    .map_err(|err| err.to_string())?;
                if let Some(device) = self.doc.devices.get_mut(device_id) {
                    device.state_ref = Some(StateBlobRef {
                        hash: device_id.to_string(),
                        encoding: PLUGIN_STATE_ENCODING.into(),
                    });
                }
            }
        }
        if !self.pending_import_dirs.is_empty() {
            let assets_dir = path.join("assets");
            std::fs::create_dir_all(&assets_dir).map_err(|err| err.to_string())?;
            for (asset_id, dir) in &self.pending_import_dirs {
                let Some(entry) = self.doc.assets.assets.get(asset_id) else {
                    continue;
                };
                let file_name = Path::new(&entry.uri).file_name().map_or_else(
                    || entry.uri.clone(),
                    |name| name.to_string_lossy().into_owned(),
                );
                std::fs::copy(dir.join(&entry.uri), assets_dir.join(file_name))
                    .map_err(|err| err.to_string())?;
            }
        }
        notwig_project::save(&self.doc, &path).map_err(|err| err.to_string())?;
        for dir in self.pending_import_dirs.values() {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.pending_import_dirs.clear();
        self.bundle = Some(path.clone());
        if previous.as_deref() != Some(path.as_path())
            && let Some(previous) = previous
            && let Err(err) = notwig_project::remove_session_lock(&previous)
        {
            tracing::warn!(
                path = %previous.display(),
                error = %err,
                "could not remove old session lock"
            );
        }
        self.dirty = false;
        self.begin_session();
        self.status = Some(format!("Saved {}", path.display()));
        Ok(path)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn autosave_if_due(&mut self) {
        // Browser builds persist through the OPFS project store instead of
        // native autosave snapshots.
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save(&mut self, _as_path: Option<&Path>) -> Result<PathBuf, String> {
        Err("use the browser project store".into())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn bundle_files(&mut self) -> Result<notwig_project::BundleFiles, String> {
        let mut files = notwig_project::BundleFiles::default();
        let asset_ids: Vec<notwig_core::AssetId> = self.doc.assets.assets.keys().copied().collect();
        for asset_id in asset_ids {
            let Some(entry) = self.doc.assets.assets.get(&asset_id).cloned() else {
                continue;
            };
            if entry.kind != notwig_core::AssetKind::Audio {
                continue;
            }
            let Some(buffer) = self.assets.get(&asset_id) else {
                continue;
            };
            let uri = format!("assets/{asset_id}.wav");
            let bytes = notwig_project::write_wav_f32_bytes(
                buffer.sample_rate,
                buffer.channels,
                &buffer.samples,
            )
            .map_err(|err| err.to_string())?;
            files.insert(uri.clone(), bytes);
            if let Some(entry) = self.doc.assets.assets.get_mut(&asset_id) {
                entry.uri = uri;
                entry.format = "wav".into();
                entry.sample_rate = Some(buffer.sample_rate);
                entry.channels = Some(buffer.channels);
                entry.duration_samples = Some(buffer.frame_count());
            }
        }
        for (device_id, state) in &self.plugin_states {
            let bytes = serde_json::to_vec(state).map_err(|err| err.to_string())?;
            files.insert(format!("state/{device_id}.json"), bytes);
        }
        notwig_project::save_files(&self.doc, files).map_err(|err| err.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_bundle_files(
        files: &notwig_project::BundleFiles,
        label: String,
    ) -> Result<Self, String> {
        let loaded = notwig_project::open_files(files).map_err(|err| err.to_string())?;
        let mut workspace = Self::from_doc(loaded.doc, Some(PathBuf::from(label)), false);
        workspace.load_assets_from_files(files);
        workspace.request_engine_rebuild();
        if !loaded.issues.is_empty() {
            workspace.status = Some(format!(
                "Project loaded with {} issue(s).",
                loaded.issues.len()
            ));
        }
        Ok(workspace)
    }

    /// Native-only autosave. Browser builds persist through the OPFS project
    /// store and never create autosave snapshots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn autosave_if_due(&mut self) {
        if !self.dirty || self.last_autosave.elapsed().as_secs() < 60 {
            return;
        }
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        if !self.pending_audio_imports.is_empty() {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            });
        match notwig_project::autosave(&self.doc, &bundle, timestamp) {
            Ok(path) => {
                self.last_autosave = Instant::now();
                tracing::debug!(path = %path.display(), "autosaved project");
            }
            Err(err) => {
                // Keep the previous deadline so the next tick retries instead
                // of silently waiting another full interval.
                tracing::warn!(
                    path = %bundle.display(),
                    error = %err,
                    "autosave failed"
                );
                self.status = Some(format!("Autosave failed: {err}"));
            }
        }
    }
}
