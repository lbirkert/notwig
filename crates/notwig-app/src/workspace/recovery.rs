use super::Workspace;

impl Workspace {
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
    }

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
        self.request_engine_rebuild();
        self.last_autosave = std::time::Instant::now();
        self.error = None;
        self.status = Some("Recovered latest autosave.".into());
        Ok(())
    }

    pub fn discard_recovery(&mut self) -> Result<(), String> {
        let Some(bundle) = self.bundle.clone() else {
            self.recovery_offered = false;
            return Ok(());
        };
        notwig_project::cleanup_autosaves(&bundle).map_err(|err| err.to_string())?;
        notwig_project::remove_session_lock(&bundle).map_err(|err| err.to_string())?;
        self.recovery_offered = false;
        self.status = Some("Discarded recovery autosaves.".into());
        Ok(())
    }
}
