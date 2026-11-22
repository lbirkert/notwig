use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use notwig_core::dsp::{DeviceFactory, DeviceSpec};
use notwig_ext::{
    ExtensionHost, InstalledExtension, install_zip_bytes, install_zip_file, uninstall,
};
use notwig_ext_api::{DevicePanelHost, ExtensionConfig, ExtensionIndexEntry, ExtensionInfo};

use super::ExtensionEvent;
use super::index::{
    IndexFetchResult, MAX_INDEX_BYTES, merge_indexes, normalize_indexes, select_download,
    verify_sha256,
};

const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ICON_BYTES: usize = 4 * 1024 * 1024;

type InstallResult = Result<InstalledExtension, String>;

pub(crate) struct ExtensionManager {
    config: ExtensionConfig,
    host: Option<ExtensionHost>,
    infos: Vec<ExtensionInfo>,
    index_entries: Vec<ExtensionIndexEntry>,
    index_icons: BTreeMap<String, Arc<Vec<u8>>>,
    index_errors: Vec<String>,
    index_loading: bool,
    index_loaded: bool,
    error: Option<String>,
    index_rx: Option<mpsc::Receiver<IndexFetchResult>>,
    install_rx: Option<mpsc::Receiver<InstallResult>>,
}

impl ExtensionManager {
    pub(crate) fn load() -> Self {
        let config = load_config();
        let mut manager = Self {
            config,
            host: None,
            infos: Vec::new(),
            index_entries: Vec::new(),
            index_icons: BTreeMap::new(),
            index_errors: Vec::new(),
            index_loading: false,
            index_loaded: false,
            error: None,
            index_rx: None,
            install_rx: None,
        };
        manager.reload();
        manager
    }

    pub(crate) fn reload(&mut self) {
        let Some(root) = extensions_dir() else {
            self.error = Some("Could not locate the extensions directory.".into());
            return;
        };
        match ExtensionHost::load(&root, &self.config.settings) {
            Ok(host) => {
                self.infos = host.infos().to_vec();
                self.host = Some(host);
                self.error = None;
            }
            Err(error) => {
                self.infos.clear();
                self.host = None;
                self.error = Some(error.to_string());
            }
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<ExtensionEvent> {
        let mut events = Vec::new();
        self.poll_index();
        if let Some(receiver) = self.install_rx.as_ref() {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Extension install worker stopped.".into()))
                }
            };
            if let Some(result) = result {
                self.install_rx = None;
                match result {
                    Ok(installed) => {
                        self.reload();
                        events.push(ExtensionEvent::Installed {
                            name: installed.manifest.name,
                            version: installed.manifest.version,
                        });
                    }
                    Err(error) => events.push(ExtensionEvent::Error(error)),
                }
            }
        }
        events
    }

    pub(crate) fn busy(&self) -> bool {
        self.index_loading || self.index_rx.is_some() || self.install_rx.is_some()
    }

    fn poll_index(&mut self) {
        let result = self
            .index_rx
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(IndexFetchResult {
                    entries: Vec::new(),
                    errors: vec!["Extension index worker stopped.".into()],
                    images: BTreeMap::new(),
                }),
            });
        if let Some(result) = result {
            self.index_rx = None;
            self.index_entries = result.entries;
            self.index_icons = result
                .images
                .into_iter()
                .map(|(id, bytes)| (id, Arc::new(bytes)))
                .collect();
            self.index_errors = result.errors;
            self.index_loading = false;
            self.index_loaded = true;
        }
    }

    pub(crate) fn specs(&self) -> Vec<DeviceSpec> {
        self.host
            .as_ref()
            .map(ExtensionHost::specs)
            .unwrap_or_default()
    }

    pub(crate) fn factory(&self) -> Option<Arc<dyn DeviceFactory>> {
        self.host.as_ref().map(ExtensionHost::factory)
    }

    pub(crate) fn panel_host(&self) -> Option<Box<dyn DevicePanelHost>> {
        self.host.as_ref().map(ExtensionHost::panel_host)
    }

    #[allow(dead_code)]
    pub(crate) fn engine_runtime(&self) -> Option<crate::engine_build::EngineExtensionRuntime> {
        Some(crate::engine_build::EngineExtensionRuntime {
            root: extensions_dir()?,
            settings: self.config.settings.clone(),
        })
    }

    pub(crate) fn infos(&self) -> Vec<ExtensionInfo> {
        self.infos.clone()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn errors(&self) -> Vec<String> {
        self.error.iter().cloned().collect()
    }

    pub(crate) fn icons(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.index_icons.clone()
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
        self.index_loaded = false;
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
        std::thread::spawn(move || {
            let _ = sender.send(fetch_indexes(indexes));
        });
    }

    pub(crate) fn install_from_index(&mut self, id: &str) -> Result<(), String> {
        if self.install_rx.is_some() {
            return Err("An extension install is already running.".into());
        }
        let entry = self
            .index_entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| format!("Extension {id} is not in the current index."))?;
        let download = select_download(&entry)
            .cloned()
            .ok_or_else(|| format!("Extension {id} has no package for this platform."))?;
        let Some(root) = extensions_dir() else {
            return Err("Could not locate the extensions directory.".into());
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = read_source_bytes(&download.url).and_then(|bytes| {
                verify_sha256(&bytes, &download.hash)?;
                install_zip_bytes(&root, &bytes, Some(&entry.id)).map_err(|error| error.to_string())
            });
            let _ = sender.send(result);
        });
        self.install_rx = Some(receiver);
        Ok(())
    }

    pub(crate) fn install_zip(&mut self, path: PathBuf) -> Result<(), String> {
        if self.install_rx.is_some() {
            return Err("An extension install is already running.".into());
        }
        let Some(root) = extensions_dir() else {
            return Err("Could not locate the extensions directory.".into());
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = install_zip_file(&root, &path, None).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.install_rx = Some(receiver);
        Ok(())
    }

    pub(crate) fn uninstall(&mut self, id: &str) -> Result<(), String> {
        let Some(root) = extensions_dir() else {
            return Err("Could not locate the extensions directory.".into());
        };
        uninstall(&root, id).map_err(|error| error.to_string())?;
        self.config.settings.remove(id);
        self.persist();
        self.reload();
        Ok(())
    }

    pub(crate) fn set_enabled(&mut self, id: &str, enabled: bool) {
        let entry = self.config.settings.entry(id.to_owned()).or_default();
        entry.enabled = enabled;
        self.persist();
        self.reload();
    }

    fn persist(&self) {
        if let Err(error) = save_config(&self.config) {
            tracing::warn!(error = %error, "could not save extension settings");
        }
    }
}

fn fetch_indexes(indexes: Vec<String>) -> IndexFetchResult {
    let mut documents = Vec::new();
    let mut errors = Vec::new();
    for source in indexes {
        match read_source_bytes(&source) {
            Ok(bytes) if bytes.len() > MAX_INDEX_BYTES => {
                errors.push(format!("{source}: index is too large"));
            }
            Ok(bytes) => documents.push((source, bytes)),
            Err(error) => errors.push(error),
        }
    }
    let mut result = merge_indexes(documents);
    result.errors.splice(0..0, errors);
    result.images = fetch_index_images(&result.entries);
    result
}

fn fetch_index_images(entries: &[ExtensionIndexEntry]) -> BTreeMap<String, Vec<u8>> {
    let mut images = BTreeMap::new();
    for entry in entries {
        let Some(image) = &entry.image else {
            continue;
        };
        match read_source_bytes(image) {
            Ok(bytes) if bytes.len() <= MAX_ICON_BYTES => {
                images.insert(entry.id.clone(), bytes);
            }
            Ok(_) => {
                tracing::warn!(extension = %entry.id, image, "extension index icon is too large");
            }
            Err(error) => {
                tracing::warn!(extension = %entry.id, image, error = %error, "could not load extension index icon");
            }
        }
    }
    images
}

fn read_source_bytes(source: &str) -> Result<Vec<u8>, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        return fetch_url(source);
    }
    if source.starts_with("file://") {
        let url = url::Url::parse(source).map_err(|error| format!("{source}: {error}"))?;
        let path = url
            .to_file_path()
            .map_err(|()| format!("{source}: invalid file URL"))?;
        return std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()));
    }
    std::fs::read(source).map_err(|error| format!("{source}: {error}"))
}

fn fetch_url(source: &str) -> Result<Vec<u8>, String> {
    let mut response = ureq::get(source)
        .call()
        .map_err(|error| format!("{source}: {error}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_ARCHIVE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{source}: {error}"))?;
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(format!("{source}: response is too large"));
    }
    Ok(bytes)
}

fn extensions_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/notwig/extensions");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".config/notwig/extensions");
    Some(path)
}

fn config_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/notwig/extensions.json");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".config/notwig/extensions.json");
    Some(path)
}

fn load_config() -> ExtensionConfig {
    let Some(path) = config_path() else {
        return ExtensionConfig::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ExtensionConfig::default();
    };
    ExtensionConfig::from_json(&text).unwrap_or_else(|error| {
        tracing::warn!(path = %path.display(), error = %error, "ignoring invalid extension config");
        ExtensionConfig::default()
    })
}

fn save_config(config: &ExtensionConfig) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = config.to_json()?;
    std::fs::write(&path, text).map_err(|error| format!("save extension config: {error}"))
}
