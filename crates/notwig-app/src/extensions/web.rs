#![allow(clippy::unused_self)]

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use notwig_dsp::{DeviceFactory, DeviceSpec};
use notwig_ext::{
    ExtensionArchive, WebExtensionHost, WebExtensionPackage, read_archive, validate_extension_id,
};
use notwig_ext_api::{DevicePanelHost, ExtensionConfig, ExtensionIndexEntry, ExtensionInfo};
use wasm_bindgen_futures::spawn_local;

use super::ExtensionEvent;
use super::index::{
    IndexFetchResult, MAX_INDEX_BYTES, merge_indexes, normalize_indexes, select_download,
    verify_sha256,
};
use crate::web::storage::OpfsEntryKind;

const EXTENSIONS_DIR: &str = "notwig-extensions";
const CONFIG_KEY: &str = "notwig.extension-config";

pub(crate) struct ExtensionManager {
    config: ExtensionConfig,
    host: Option<WebExtensionHost>,
    infos: Vec<ExtensionInfo>,
    error: Option<String>,
    loading: bool,
    index_entries: Vec<ExtensionIndexEntry>,
    index_errors: Vec<String>,
    index_loading: bool,
    index_loaded: bool,
    load_rx: Option<mpsc::Receiver<LoadOutcome>>,
    index_rx: Option<mpsc::Receiver<IndexFetchResult>>,
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
        let config = load_config();
        let mut manager = Self {
            config,
            host: None,
            infos: Vec::new(),
            error: None,
            loading: false,
            index_entries: Vec::new(),
            index_errors: Vec::new(),
            index_loading: false,
            index_loaded: false,
            load_rx: None,
            index_rx: None,
            admin_rx: None,
        };
        manager.reload();
        manager.start_index_fetch();
        manager
    }

    fn start_load(&mut self) {
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

    pub(crate) fn busy(&self) -> bool {
        self.load_rx.is_some() || self.index_rx.is_some() || self.admin_rx.is_some()
    }

    pub(crate) fn poll(&mut self) -> Vec<ExtensionEvent> {
        let mut events = Vec::new();
        if self.poll_load() {
            events.push(ExtensionEvent::Loaded);
        }
        self.poll_index();
        self.poll_admin(&mut events);
        events
    }

    fn poll_load(&mut self) -> bool {
        let outcome = match self.load_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(outcome)) => outcome,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.load_rx = None;
                self.loading = false;
                self.error = Some("Extension loading stopped.".into());
                return false;
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => return false,
        };
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
        true
    }

    fn poll_index(&mut self) {
        let result = match self.index_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Disconnected)) => IndexFetchResult {
                entries: Vec::new(),
                errors: vec!["Extension index request stopped.".into()],
            },
            Some(Err(mpsc::TryRecvError::Empty)) | None => return,
        };
        self.index_rx = None;
        self.index_loading = false;
        self.index_loaded = true;
        self.index_entries = result.entries;
        self.index_errors = result.errors;
    }

    fn poll_admin(&mut self, events: &mut Vec<ExtensionEvent>) {
        let result = match self.admin_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                Err("Extension operation stopped.".into())
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => return,
        };
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
        self.config.indexes.clone()
    }

    pub(crate) fn index_entries(&self) -> Vec<ExtensionIndexEntry> {
        self.index_entries.clone()
    }

    pub(crate) fn index_errors(&self) -> Vec<String> {
        self.index_errors.clone()
    }

    pub(crate) fn index_loading(&self) -> bool {
        self.index_loading
    }

    pub(crate) fn index_loaded(&self) -> bool {
        self.index_loaded
    }

    pub(crate) fn set_indexes(&mut self, indexes: Vec<String>) {
        let indexes = normalize_indexes(indexes);
        if self.config.indexes == indexes {
            return;
        }
        self.config.indexes = indexes;
        self.persist();
        self.index_entries.clear();
        self.index_errors.clear();
        self.index_loading = false;
        self.index_loaded = false;
        self.index_rx = None;
        self.start_index_fetch();
    }

    pub(crate) fn refresh_index(&mut self) {
        if self.index_loading {
            return;
        }
        self.start_index_fetch();
    }

    fn start_index_fetch(&mut self) {
        if self.config.indexes.is_empty() {
            self.index_rx = None;
            self.index_entries.clear();
            self.index_errors.clear();
            self.index_loading = false;
            self.index_loaded = true;
            return;
        }
        let indexes = self.config.indexes.clone();
        self.index_loading = true;
        let (sender, receiver) = mpsc::channel();
        self.index_rx = Some(receiver);
        spawn_local(async move {
            let result = fetch_indexes(indexes).await;
            let _ = sender.send(result);
        });
    }

    pub(crate) fn install_from_index(&mut self, id: &str) -> Result<(), String> {
        if self.admin_rx.is_some() {
            return Err("An extension operation is already running.".into());
        }
        let entry = self
            .index_entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| format!("Extension {id} is not in the current index."))?;
        let download = select_download(&entry)
            .cloned()
            .ok_or_else(|| format!("Extension {id} has no WASM package."))?;
        let (sender, receiver) = mpsc::channel();
        self.admin_rx = Some(receiver);
        spawn_local(async move {
            let result = async {
                let bytes = crate::web::storage::fetch_bytes(&download.url)
                    .await?
                    .ok_or_else(|| format!("{}: not found", download.url))?;
                verify_sha256(&bytes, &download.hash)?;
                let archive = read_archive(&bytes, Some(entry.id.as_str()))
                    .map_err(|error| error.to_string())?;
                let package = WebExtensionPackage::from_archive(archive.clone())
                    .map_err(|error| error.to_string())?;
                let name = package.name().to_owned();
                let version = package.version().to_owned();
                install_archive(package.id(), &archive).await?;
                Ok(AdminEvent::Installed { name, version })
            }
            .await;
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub(crate) fn install_zip(&mut self, _path: PathBuf) -> Result<(), String> {
        Err("Use the browser ZIP picker to install a WASM extension.".into())
    }

    pub(crate) fn install_zip_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if self.admin_rx.is_some() {
            return Err("An extension operation is already running.".into());
        }
        let archive = read_archive(&bytes, None).map_err(|error| error.to_string())?;
        let package = WebExtensionPackage::from_archive(archive.clone())
            .map_err(|error| error.to_string())?;
        let id = package.id().to_owned();
        let name = package.name().to_owned();
        let version = package.version().to_owned();
        let (sender, receiver) = mpsc::channel();
        self.admin_rx = Some(receiver);
        spawn_local(async move {
            let result = install_archive(&id, &archive)
                .await
                .map(|()| AdminEvent::Installed { name, version });
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub(crate) fn uninstall(&mut self, id: &str) -> Result<(), String> {
        if self.admin_rx.is_some() {
            return Err("An extension operation is already running.".into());
        }
        validate_extension_id(id).map_err(|error| error.to_string())?;
        self.config.settings.remove(id);
        self.persist();
        let id = id.to_owned();
        let (sender, receiver) = mpsc::channel();
        self.admin_rx = Some(receiver);
        spawn_local(async move {
            let result = crate::web::storage::remove_opfs_path(&extension_dir(&id))
                .await
                .map(|()| AdminEvent::Removed { id });
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub(crate) fn set_enabled(&mut self, _id: &str, _enabled: bool) {
        // Browser extensions are always enabled.
    }

    fn persist(&self) {
        let text = match self.config.to_json() {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(error = %error, "could not encode extension config");
                return;
            }
        };
        let Some(storage) =
            web_sys::window().and_then(|window| window.local_storage().ok().flatten())
        else {
            return;
        };
        if let Err(error) = storage.set_item(CONFIG_KEY, &text) {
            tracing::warn!(error = ?error, "could not save extension config");
        }
    }
}

async fn load_packages() -> (Vec<WebExtensionPackage>, Vec<String>) {
    let mut packages = Vec::new();
    let mut errors = Vec::new();
    let entries = match crate::web::storage::list_opfs_entries(EXTENSIONS_DIR).await {
        Ok(entries) => entries,
        Err(error) => return (packages, vec![format!("{EXTENSIONS_DIR}: {error}")]),
    };
    for (name, kind) in entries {
        match kind {
            OpfsEntryKind::Directory => {
                let path = format!("{EXTENSIONS_DIR}/{name}");
                match load_directory(&path).await {
                    Ok(package) => packages.push(package),
                    Err(error) => errors.push(format!("{path}: {error}")),
                }
            }
            OpfsEntryKind::File if is_zip_name(&name) => {
                let path = format!("{EXTENSIONS_DIR}/{name}");
                match migrate_zip(&path).await {
                    Ok(Some(package)) => packages.push(package),
                    Ok(None) => {}
                    Err(error) => errors.push(format!("{path}: {error}")),
                }
            }
            OpfsEntryKind::File => {}
        }
    }
    packages.sort_by(|left, right| left.id().cmp(right.id()));
    (packages, errors)
}

async fn load_directory(path: &str) -> Result<WebExtensionPackage, String> {
    let files = crate::web::storage::read_opfs_directory_files(path)
        .await?
        .ok_or_else(|| "extension directory is missing".to_string())?;
    let files = files
        .into_iter()
        .map(|(path, bytes)| (PathBuf::from(path), bytes))
        .collect();
    WebExtensionPackage::from_files(files).map_err(|error| error.to_string())
}

async fn migrate_zip(path: &str) -> Result<Option<WebExtensionPackage>, String> {
    let Some(bytes) = crate::web::storage::read_opfs_optional(path).await? else {
        return Ok(None);
    };
    let archive = read_archive(&bytes, None).map_err(|error| error.to_string())?;
    let package =
        WebExtensionPackage::from_archive(archive.clone()).map_err(|error| error.to_string())?;
    install_archive(package.id(), &archive).await?;
    if let Err(error) = crate::web::storage::remove_opfs_path(path).await {
        tracing::warn!(path = %path, error = %error, "could not remove migrated extension ZIP");
    }
    Ok(Some(package))
}

async fn install_archive(id: &str, archive: &ExtensionArchive) -> Result<(), String> {
    validate_extension_id(id).map_err(|error| error.to_string())?;
    let dir = extension_dir(id);
    crate::web::storage::remove_opfs_path(&dir).await?;
    let files: BTreeMap<String, Vec<u8>> = archive
        .files
        .iter()
        .map(|(path, bytes)| (opfs_path(path), bytes.clone()))
        .collect();
    if let Err(error) = crate::web::storage::write_opfs_files(&dir, &files).await {
        let _ = crate::web::storage::remove_opfs_path(&dir).await;
        return Err(error);
    }
    Ok(())
}

async fn fetch_indexes(indexes: Vec<String>) -> IndexFetchResult {
    let mut documents = Vec::new();
    let mut errors = Vec::new();
    for source in indexes {
        match crate::web::storage::fetch_bytes(&source).await {
            Ok(Some(bytes)) if bytes.len() > MAX_INDEX_BYTES => {
                errors.push(format!("{source}: index is too large"));
            }
            Ok(Some(bytes)) => documents.push((source, bytes)),
            Ok(None) => errors.push(format!("{source}: not found")),
            Err(error) => errors.push(format!("{source}: {error}")),
        }
    }
    let mut result = merge_indexes(documents);
    result.errors.splice(0..0, errors);
    result
}

fn extension_dir(id: &str) -> String {
    format!("{EXTENSIONS_DIR}/{id}")
}

fn is_zip_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn opfs_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn load_config() -> ExtensionConfig {
    let Some(text) = web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(CONFIG_KEY).ok().flatten())
    else {
        return ExtensionConfig::default();
    };
    ExtensionConfig::from_json(&text).unwrap_or_else(|error| {
        tracing::warn!(error = %error, "ignoring invalid extension config");
        ExtensionConfig::default()
    })
}
