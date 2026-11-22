use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use notwig_core::DeviceKind;
use notwig_plugins::{
    PluginCache, PluginDescriptor, PluginError, PluginHost, PluginMetadata, PluginProbe,
};
use notwig_ui::PluginEntry;

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

type ProbeResult = Result<PluginMetadata, PluginError>;
static PROBE_CACHE: OnceLock<Mutex<BTreeMap<String, ProbeResult>>> = OnceLock::new();

fn probe_cache() -> &'static Mutex<BTreeMap<String, ProbeResult>> {
    PROBE_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn probe_key(descriptor: &PluginDescriptor) -> String {
    format!(
        "{:?}:{}:{}:{}:{}",
        descriptor.format,
        descriptor.type_id,
        descriptor.path.display(),
        descriptor.modified_utc,
        descriptor.binary_size
    )
}

pub(crate) fn new_plugin_host() -> PluginHost {
    let mut host = PluginHost::new();
    host.set_probe(Arc::new(SubprocessProbe));
    host
}

/// Loads plugin metadata in a child process.
///
/// A crashing native plugin only takes down the probe process. The host process
/// records the failure and never calls into that plugin.
struct SubprocessProbe;

impl PluginProbe for SubprocessProbe {
    fn probe(&self, descriptor: &PluginDescriptor) -> Result<PluginMetadata, PluginError> {
        let key = probe_key(descriptor);
        if let Ok(cache) = probe_cache().lock()
            && let Some(result) = cache.get(&key)
        {
            return result.clone();
        }
        let result = Self::probe_uncached(descriptor);
        if let Ok(mut cache) = probe_cache().lock() {
            cache.insert(key, result.clone());
        }
        result
    }
}

impl SubprocessProbe {
    fn probe_uncached(descriptor: &PluginDescriptor) -> Result<PluginMetadata, PluginError> {
        let exe = std::env::current_exe().map_err(|err| PluginError::Io(err.to_string()))?;
        let input = serde_json::to_vec(descriptor)
            .map_err(|err| PluginError::Load(format!("encode plugin descriptor: {err}")))?;
        let mut child = Command::new(exe)
            .arg("load-plugin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| PluginError::Io(format!("start plugin probe: {err}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&input)
                .map_err(|err| PluginError::Io(format!("write plugin probe input: {err}")))?;
        }
        let reader = child.stdout.take().map(|mut stdout| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = stdout.read_to_end(&mut bytes);
                bytes
            })
        });
        let deadline = Instant::now() + PROBE_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(PluginError::Load(format!(
                            "plugin probe timed out: {}",
                            descriptor.name
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PluginError::Io(format!("wait for plugin probe: {err}")));
                }
            }
        };
        let output = reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();
        if !status.success() {
            return Err(PluginError::Load(format!(
                "plugin probe failed for {}: {status}",
                descriptor.name
            )));
        }
        let metadata = output
            .split(|byte| *byte == b'\n')
            .rev()
            .find_map(|line| line.strip_prefix(notwig_plugins::PLUGIN_METADATA_PREFIX.as_bytes()))
            .ok_or_else(|| {
                PluginError::Load(format!(
                    "plugin probe did not report metadata for {}",
                    descriptor.name
                ))
            })?;
        serde_json::from_slice(metadata).map_err(|err| {
            PluginError::Load(format!(
                "invalid plugin metadata for {}: {err}",
                descriptor.name
            ))
        })
    }
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
