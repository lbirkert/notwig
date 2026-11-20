#![allow(clippy::unused_self, clippy::unnecessary_wraps)]

use std::path::PathBuf;
use std::sync::Arc;

use notwig_dsp::{DeviceFactory, DeviceSpec};
use notwig_ext_api::{DevicePanelHost, ExtensionIndexEntry, ExtensionInfo, TrustLevel};

use super::ExtensionEvent;

pub(crate) struct ExtensionManager;

impl ExtensionManager {
    pub(crate) fn load() -> Self {
        Self
    }

    pub(crate) fn reload(&mut self) {}

    pub(crate) fn poll(&mut self) -> Vec<ExtensionEvent> {
        Vec::new()
    }

    pub(crate) fn specs(&self) -> Vec<DeviceSpec> {
        Vec::new()
    }

    pub(crate) fn factory(&self) -> Option<Arc<dyn DeviceFactory>> {
        None
    }

    pub(crate) fn panel_host(&self) -> Option<Box<dyn DevicePanelHost>> {
        None
    }

    pub(crate) fn infos(&self) -> Vec<ExtensionInfo> {
        Vec::new()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        Some("Extensions are not available in the browser build.")
    }

    pub(crate) fn indexes(&self) -> Vec<String> {
        Vec::new()
    }

    pub(crate) fn index_entries(&self) -> Vec<ExtensionIndexEntry> {
        Vec::new()
    }

    pub(crate) fn index_errors(&self) -> Vec<String> {
        Vec::new()
    }

    pub(crate) fn index_loading(&self) -> bool {
        false
    }

    pub(crate) fn index_loaded(&self) -> bool {
        true
    }

    pub(crate) fn set_indexes(&mut self, _indexes: Vec<String>) {}

    pub(crate) fn refresh_index(&mut self) {}

    pub(crate) fn install_from_index(&mut self, _id: &str) -> Result<(), String> {
        Err("Extensions are not available in the browser build.".into())
    }

    pub(crate) fn install_zip(&mut self, _path: PathBuf) -> Result<(), String> {
        Err("Extensions are not available in the browser build.".into())
    }

    pub(crate) fn uninstall(&mut self, _id: &str) -> Result<(), String> {
        Err("Extensions are not available in the browser build.".into())
    }

    pub(crate) fn set_enabled(&mut self, _id: &str, _enabled: bool) {}

    pub(crate) fn set_trust(&mut self, _id: &str, _trust: TrustLevel) {}
}
