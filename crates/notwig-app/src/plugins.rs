use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, mpsc};

use notwig_core::DeviceKind;
use notwig_plugins::{PluginCache, PluginDescriptor, PluginHost};
use notwig_ui::PluginEntry;

pub(crate) fn new_plugin_host() -> PluginHost {
    PluginHost::new()
}

pub(crate) struct PluginCatalog {
    descriptors: Mutex<Vec<PluginDescriptor>>,
    scan: Mutex<Option<mpsc::Receiver<Vec<PluginDescriptor>>>>,
}

impl PluginCatalog {
    pub(crate) fn global() -> &'static Self {
        static CATALOG: OnceLock<PluginCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            if cfg!(test) {
                return Self {
                    descriptors: Mutex::new(Vec::new()),
                    scan: Mutex::new(None),
                };
            }
            let cache = plugin_cache_path()
                .and_then(|path| PluginCache::load(&path).ok())
                .map(|cache| cache.descriptors())
                .unwrap_or_default();
            tracing::info!(count = cache.len(), "loaded cached audio plugin registry");
            let (tx, rx) = mpsc::channel();
            let cache_path = plugin_cache_path();
            std::thread::spawn(move || {
                if let Some(path) = cache_path {
                    if let Ok(exe) = std::env::current_exe() {
                        let status = std::process::Command::new(exe)
                            .arg("scan-plugins")
                            .arg(&path)
                            .status();
                        if !status.is_ok_and(|status| status.success()) {
                            tracing::warn!("plugin scanner process failed");
                        }
                    }
                    let descriptors = PluginCache::load(&path)
                        .ok()
                        .map(|cache| cache.descriptors())
                        .unwrap_or_default();
                    tracing::info!(count = descriptors.len(), "scanned audio plugins");
                    let _ = tx.send(descriptors);
                }
            });
            Self {
                descriptors: Mutex::new(cache),
                scan: Mutex::new(Some(rx)),
            }
        })
    }

    pub(crate) fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.descriptors
            .lock()
            .map_or_else(|_| Vec::new(), |descriptors| descriptors.clone())
    }

    pub(crate) fn poll(&self) -> bool {
        let Ok(mut scan) = self.scan.lock() else {
            return false;
        };
        let Some(receiver) = scan.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(descriptors) => {
                if let Ok(mut current) = self.descriptors.lock() {
                    *current = descriptors;
                }
                *scan = None;
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                *scan = None;
                false
            }
        }
    }
}

fn plugin_cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = PathBuf::from(home);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Caches/notwig/plugin-cache.json");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".cache/notwig/plugin-cache.json");
    Some(path)
}

pub(crate) fn plugin_entries(descriptors: &[PluginDescriptor]) -> Vec<PluginEntry> {
    descriptors
        .iter()
        .map(|descriptor| PluginEntry {
            type_id: descriptor.type_id.clone(),
            name: descriptor.name.clone(),
            vendor: descriptor.vendor.clone(),
            is_instrument: descriptor.is_instrument,
            kind: if descriptor.is_instrument {
                DeviceKind::Instrument
            } else {
                DeviceKind::AudioFx
            },
        })
        .collect()
}
