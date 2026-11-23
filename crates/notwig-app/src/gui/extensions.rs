use std::path::PathBuf;

use crate::clock::Duration;

use super::NotwigApp;

impl NotwigApp {
    pub(super) fn sync_extensions(&mut self) {
        self.workspace
            .set_extension_devices(self.extensions.specs(), self.extensions.factory());
        self.workspace
            .set_extension_engine_runtime(self.extensions.engine_runtime());
        self.ui.set_device_panel_host(self.extensions.panel_host());
    }

    pub(super) fn poll_extension_events(&mut self, ctx: &eframe::egui::Context) {
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
    }

    pub(super) fn set_extension_enabled(&mut self, id: &str, enabled: bool) {
        self.extensions.set_enabled(id, enabled);
        self.sync_extensions();
    }

    pub(super) fn reload_extensions(&mut self) {
        self.extensions.reload();
        self.sync_extensions();
        if let Some(error) = self.extensions.error() {
            self.notify_error(format!("Extension load failed: {error}"));
        } else {
            self.notify_info("Extensions reloaded.".into());
        }
    }

    pub(super) fn set_extension_indexes(&mut self, indexes: Vec<String>) {
        self.extensions.set_indexes(indexes);
    }

    pub(super) fn refresh_extension_index(&mut self) {
        self.extensions.refresh_index();
    }

    pub(super) fn install_extension_from_index(&mut self, id: &str) {
        if let Err(error) = self.extensions.install_from_index(id) {
            self.notify_error(error);
        }
    }

    pub(super) fn install_extension_zip(&mut self, path: PathBuf) {
        if let Err(error) = self.extensions.install_zip(path) {
            self.notify_error(error);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn install_extension_zip_bytes(&mut self, name: String, bytes: &[u8]) {
        match self.extensions.install_zip_bytes(bytes.to_vec()) {
            Ok(()) => self.notify_info(format!("Installing {name}…")),
            Err(error) => self.notify_error(error),
        }
    }

    pub(super) fn uninstall_extension(&mut self, id: &str) {
        match self.extensions.uninstall(id) {
            Ok(()) => {
                self.sync_extensions();
                self.notify_info(format!("Uninstalled {id}."));
            }
            Err(error) => self.notify_error(error),
        }
    }
}
