#![allow(clippy::unused_self)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use notwig_dsp::{DeviceFactory, DeviceSpec};
use notwig_ext::{WebExtensionHost, WebExtensionPackage};
use notwig_ext_api::{DevicePanelHost, ExtensionIndexEntry, ExtensionInfo, TrustLevel};
use wasm_bindgen_futures::spawn_local;

use super::ExtensionEvent;

const INDEX_PATH: &str = "notwig-extensions/index.json";
const STOCK_URLS: [&str; 2] = [
    "../extensions/notwig.ext.devices.zip",
    "/extensions/notwig.ext.devices.zip",
];

pub(crate) struct ExtensionManager {
    host: Option<WebExtensionHost>,
    infos: Vec<ExtensionInfo>,
    error: Option<String>,
    loading: bool,
    load_rx: Option<mpsc::Receiver<LoadOutcome>>,
    admin_rx: Option<mpsc::Receiver<Result<AdminEvent, String>>>,
}

struct LoadOutcome {
    host: WebExtensionHost,
    errors: Vec<String>,
}

enum AdminEvent {
    Installed { name: String, version: String },
    Removed { id: String },
}

impl ExtensionManager {
    pub(crate) fn load() -> Self {
        let mut manager = Self {
            host: None,
            infos: Vec::new(),
            error: None,
            loading: true,
            load_rx: None,
            admin_rx: None,
        };
        manager.start_load();
        manager
    }

    fn start_load(&mut self) {
        if self.load_rx.is_some() {
            return;
        }
        self.loading = true;
        let (sender, receiver) = mpsc::channel();
        self.load_rx = Some(receiver);
        spawn_local(async move {
            let (packages, errors) = load_packages().await;
            let host = WebExtensionHost::load(packages);
            let _ = sender.send(LoadOutcome { host, errors });
        });
    }

    pub(crate) fn reload(&mut self) {
        self.start_load();
    }

    pub(crate) fn poll(&mut self) -> Vec<ExtensionEvent> {
        let mut events = Vec::new();
        if let Some(receiver) = self.load_rx.as_ref() {
            if let Ok(outcome) = receiver.try_recv() {
                self.load_rx = None;
                self.loading = false;
                tracing::info!(
                    count = outcome.host.infos().len(),
                    errors = outcome.errors.len(),
                    "loaded browser extensions"
                );
                self.infos = outcome.host.infos().to_vec();
                self.host = Some(outcome.host);
                self.error = if outcome.errors.is_empty() {
                    None
                } else {
                    Some(outcome.errors.join("; "))
                };
            }
        }
        if let Some(receiver) = self.admin_rx.as_ref() {
            if let Ok(result) = receiver.try_recv() {
                self.admin_rx = None;
                match result {
                    Ok(AdminEvent::Installed { name, version }) => {
                        events.push(ExtensionEvent::Installed { name, version });
                        self.start_load();
                    }
                    Ok(AdminEvent::Removed { id }) => {
                        self.infos.retain(|info| info.id != id);
                        self.start_load();
                    }
                    Err(error) => events.push(ExtensionEvent::Error(error)),
                }
            }
        }
        events
    }

    pub(crate) fn specs(&self) -> Vec<DeviceSpec> {
        self.host
            .as_ref()
            .map(WebExtensionHost::specs)
            .unwrap_or_default()
    }

    pub(crate) fn factory(&self) -> Option<Arc<dyn DeviceFactory>> {
        self.host.as_ref().map(WebExtensionHost::factory)
    }

    pub(crate) fn panel_host(&self) -> Option<Box<dyn DevicePanelHost>> {
        self.host.as_ref().map(WebExtensionHost::panel_host)
    }

    pub(crate) fn infos(&self) -> Vec<ExtensionInfo> {
        self.infos.clone()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
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
        self.loading
    }

    pub(crate) fn index_loaded(&self) -> bool {
        !self.loading
    }

    pub(crate) fn set_indexes(&mut self, _indexes: Vec<String>) {}

    pub(crate) fn refresh_index(&mut self) {}

    pub(crate) fn install_from_index(&mut self, _id: &str) -> Result<(), String> {
        Err("Browser builds support trusted ZIP installs only.".into())
    }

    pub(crate) fn install_zip(&mut self, _path: PathBuf) -> Result<(), String> {
        Err("Use the browser ZIP picker to install a trusted extension.".into())
    }

    pub(crate) fn install_zip_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if self.admin_rx.is_some() {
            return Err("An extension operation is already running.".into());
        }
        let package = WebExtensionPackage::from_zip(&bytes).map_err(|error| error.to_string())?;
        let id = package.id().to_owned();
        let name = package.name().to_owned();
        let version = package.version().to_owned();
        let (sender, receiver) = mpsc::channel();
        self.admin_rx = Some(receiver);
        spawn_local(async move {
            let result = async {
                let path = format!("notwig-extensions/{id}.zip");
                crate::web::storage::write_opfs(&path, &bytes).await?;
                let mut ids = read_index().await;
                if !ids.iter().any(|existing| existing == &id) {
                    ids.push(id);
                }
                write_index(&ids).await
            }
            .await;
            let _ = sender.send(result.map(|()| AdminEvent::Installed { name, version }));
        });
        Ok(())
    }

    pub(crate) fn uninstall(&mut self, id: &str) -> Result<(), String> {
        if self.admin_rx.is_some() {
            return Err("An extension operation is already running.".into());
        }
        let id = id.to_owned();
        let (sender, receiver) = mpsc::channel();
        self.admin_rx = Some(receiver);
        spawn_local(async move {
            let result = async {
                let path = format!("notwig-extensions/{id}.zip");
                let _ = crate::web::storage::remove_opfs_file(&path).await;
                let mut ids = read_index().await;
                ids.retain(|existing| existing != &id);
                write_index(&ids).await
            }
            .await;
            let _ = sender.send(result.map(|()| AdminEvent::Removed { id }));
        });
        Ok(())
    }

    pub(crate) fn set_enabled(&mut self, _id: &str, _enabled: bool) {
        // Browser extensions are always trusted and enabled.
    }

    pub(crate) fn set_trust(&mut self, _id: &str, _trust: TrustLevel) {
        // Only trusted extensions are supported.
    }
}

async fn load_packages() -> (Vec<WebExtensionPackage>, Vec<String>) {
    let mut packages = BTreeMap::new();
    let mut errors = Vec::new();

    for url in STOCK_URLS {
        match crate::web::storage::fetch_bytes(url).await {
            Ok(Some(bytes)) => match WebExtensionPackage::from_zip(&bytes) {
                Ok(package) => {
                    packages.insert(package.id().to_owned(), package);
                    break;
                }
                Err(error) => errors.push(format!("{url}: {error}")),
            },
            Ok(None) => {}
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }

    for id in read_index().await {
        let path = format!("notwig-extensions/{id}.zip");
        match crate::web::storage::read_opfs_optional(&path).await {
            Ok(Some(bytes)) => match WebExtensionPackage::from_zip(&bytes) {
                Ok(package) => {
                    packages.insert(package.id().to_owned(), package);
                }
                Err(error) => errors.push(format!("{path}: {error}")),
            },
            Ok(None) => errors.push(format!("{path}: missing")),
            Err(error) => errors.push(format!("{path}: {error}")),
        }
    }

    (packages.into_values().collect(), errors)
}

async fn read_index() -> Vec<String> {
    let Some(bytes) = crate::web::storage::read_opfs_optional(INDEX_PATH)
        .await
        .ok()
        .flatten()
    else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

async fn write_index(ids: &[String]) -> Result<(), String> {
    let bytes = serde_json::to_vec(ids).map_err(|error| error.to_string())?;
    crate::web::storage::write_opfs(INDEX_PATH, &bytes).await
}
