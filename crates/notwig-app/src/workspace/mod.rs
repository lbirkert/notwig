use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::clock::Instant;
use crate::engine_build::EngineBuild;
use crate::history::History;
use crate::plugins::{PluginCatalog, plugin_entries};
use notwig_core::dsp::{DeviceFactory, DeviceSpec, MonitorRegistry};
use notwig_core::{AssetId, AudioAssetBuffer, DeviceId, Id, Meta, ProjectDoc, SampleRate};
use notwig_engine::EngineControl;
use notwig_ext_api::{CoreExtensionRegistry, EP_NOTWIG_CLIPBOARD};
use notwig_plugins::{PluginDescriptor, PluginState};
use notwig_ui::{PluginEntry, WaveformCache};
use uuid::Uuid;

#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::EngineClientHandle;

mod assets;
mod clipboard_provider;
mod clips;
mod commands;
mod device_clipboard;
mod devices;
mod engine;
mod persistence;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod project_registry;
mod recovery;
mod render;
mod sample_rate;
mod tracks;
mod transport;

pub use render::RenderSession;
#[cfg(not(target_arch = "wasm32"))]
pub use render::StemsRenderJob;

const PLUGIN_STATE_ENCODING: &str = "notwig.plugin-state.json";
const OFFLINE_BLOCK_SIZE: usize = 4096;

pub struct Workspace {
    pub doc: ProjectDoc,
    pub bundle: Option<PathBuf>,
    pub dirty: bool,
    pub assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub waveforms: Arc<WaveformCache>,
    pending_import_dirs: BTreeMap<AssetId, PathBuf>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pending_audio_imports: std::collections::BTreeSet<AssetId>,
    plugin_states: BTreeMap<DeviceId, PluginState>,
    engine_control: EngineControl,
    engine_ready: bool,
    engine_playhead_samples: u64,
    pub plugins: Vec<PluginEntry>,
    plugin_descriptors: Vec<PluginDescriptor>,
    plugin_specs: BTreeMap<String, DeviceSpec>,
    extension_specs: BTreeMap<String, DeviceSpec>,
    extension_factory: Option<Arc<dyn DeviceFactory>>,
    monitors: Arc<MonitorRegistry>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub last_autosave: Instant,
    pub play_start: Option<notwig_core::TimeMus>,
    pub audio_block_size: u32,
    low_latency_monitoring: bool,
    pub recovery_offered: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub recovery_candidates: Vec<project_registry::RecoveryCandidate>,
    extensions: CoreExtensionRegistry,
    device_clipboard: Option<device_clipboard::DeviceClipboard>,
    history: History,
    engine_build: EngineBuild,
    #[cfg(not(target_arch = "wasm32"))]
    engine_process: Option<EngineClientHandle>,
    #[cfg(not(target_arch = "wasm32"))]
    engine_process_build: crate::engine_build::EngineProcessBuild,
    extension_runtime: Option<crate::engine_build::EngineExtensionRuntime>,
}

impl Workspace {
    pub fn new_empty() -> Result<Self, String> {
        let meta = Meta {
            name: "Untitled".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        };
        let doc = ProjectDoc::new_template(
            meta,
            Id::from_uuid(Uuid::new_v4()),
            Id::from_uuid(Uuid::new_v4()),
        )
        .map_err(|err| err.to_string())?;
        tracing::debug!("created new empty project workspace");
        Ok(Self::from_doc(doc, None, true))
    }

    pub fn open(bundle: &Path) -> Result<Self, String> {
        let started = Instant::now();
        let loaded = notwig_project::open(bundle).map_err(|err| err.to_string())?;
        let issue_count = loaded.issues.len();
        let mut workspace = Self::from_doc(loaded.doc, Some(bundle.to_path_buf()), false);
        if !loaded.issues.is_empty() {
            let summary = loaded
                .issues
                .iter()
                .map(|issue| format!("{}: {}", issue.entity, issue.message))
                .collect::<Vec<_>>()
                .join("; ");
            tracing::warn!(
                issues = issue_count,
                "project loaded with issues: {summary}"
            );
            workspace.status = Some(format!("Project loaded with issues: {summary}"));
        }
        workspace.recovery_offered = loaded.recovery_offered;
        if loaded.recovery_offered {
            workspace.status = Some("Recovery autosaves are available.".into());
        }
        workspace.begin_session();
        tracing::info!(
            path = %bundle.display(),
            elapsed_ms = started.elapsed().as_millis(),
            tracks = workspace.doc.tracks.len(),
            clips = workspace.doc.arranger.clips.len(),
            devices = workspace.doc.devices.len(),
            issues = issue_count,
            "opened project"
        );
        Ok(workspace)
    }

    fn from_doc(doc: ProjectDoc, bundle: Option<PathBuf>, dirty: bool) -> Self {
        let plugin_descriptors = PluginCatalog::global().descriptors();
        let plugins = plugin_entries(&plugin_descriptors);
        let engine_control = EngineControl::new();
        let mut workspace = Self {
            doc,
            bundle,
            dirty,
            assets: BTreeMap::new(),
            waveforms: Arc::default(),
            pending_import_dirs: BTreeMap::new(),
            pending_audio_imports: std::collections::BTreeSet::new(),
            plugin_states: BTreeMap::new(),
            engine_control,
            engine_ready: false,
            engine_playhead_samples: 0,
            plugins,
            plugin_descriptors,
            plugin_specs: BTreeMap::new(),
            extension_specs: BTreeMap::new(),
            extension_factory: None,
            monitors: Arc::new(MonitorRegistry::new()),
            status: None,
            error: None,
            last_autosave: Instant::now(),
            play_start: None,
            audio_block_size: 512,
            low_latency_monitoring: cfg!(not(target_arch = "wasm32")),
            recovery_offered: false,
            #[cfg(not(target_arch = "wasm32"))]
            recovery_candidates: Vec::new(),
            extensions: {
                let mut extensions = CoreExtensionRegistry::new();
                extensions
                    .register(
                        EP_NOTWIG_CLIPBOARD,
                        "notwig.core.clipboard",
                        std::sync::Arc::new(clipboard_provider::CoreClipboardProvider::default()),
                    )
                    .expect("core clipboard registration");
                extensions
            },
            device_clipboard: None,
            history: History::default(),
            engine_build: EngineBuild::new(),
            #[cfg(not(target_arch = "wasm32"))]
            engine_process: None,
            #[cfg(not(target_arch = "wasm32"))]
            engine_process_build: crate::engine_build::EngineProcessBuild::default(),
            extension_runtime: None,
        };
        workspace.refresh_plugin_specs();
        workspace.load_assets();
        workspace.load_plugin_states();
        workspace.request_engine_rebuild();
        workspace
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        for dir in self.pending_import_dirs.values() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
