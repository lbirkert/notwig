use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use notwig_core::dsp::{
    CompiledPlan, CompositeDeviceFactory, DeviceFactory, EngineCaps, StockDeviceFactory,
};
use notwig_core::protocol::PlaybackSchedule;
use notwig_core::{AssetId, AudioAssetBuffer, DeviceId, ProjectDoc};
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory, PreparedRuntime, TransportRuntime};
use notwig_ext_api::ExtensionSettings;
use notwig_plugins::{PluginDescriptor, PluginState};

#[cfg(not(target_arch = "wasm32"))]
use crate::engine_process::WireBuild;
use crate::plugins::new_plugin_host;

#[derive(Debug, Clone)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct EngineExtensionRuntime {
    pub(crate) root: PathBuf,
    pub(crate) settings: BTreeMap<String, ExtensionSettings>,
}

#[derive(Clone)]
pub(crate) struct EngineBuildRequest {
    pub(crate) doc: ProjectDoc,
    pub(crate) caps: EngineCaps,
    pub(crate) registry: notwig_core::dsp::DeviceRegistry,
    pub(crate) extension_factory: Option<Arc<dyn DeviceFactory>>,
    pub(crate) plugin_descriptors: Vec<PluginDescriptor>,
    pub(crate) plugin_states: BTreeMap<DeviceId, PluginState>,
    pub(crate) assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub(crate) sample_rate: u32,
    pub(crate) max_block_size: usize,
    pub(crate) monitors: Arc<notwig_core::dsp::MonitorRegistry>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) extension_runtime: Option<EngineExtensionRuntime>,
}

type BuildParts = (
    EngineConfig,
    Arc<dyn DeviceFactory>,
    Arc<CompiledPlan>,
    Arc<PlaybackSchedule>,
    BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
);

impl EngineBuildRequest {
    fn parts(self) -> Result<BuildParts, String> {
        let Self {
            doc,
            caps,
            registry,
            extension_factory,
            plugin_descriptors,
            plugin_states,
            assets,
            sample_rate,
            max_block_size,
            monitors,
            extension_runtime: _,
        } = self;
        let plan = Arc::new(
            notwig_core::dsp::compile(&doc, &registry, caps).map_err(|err| err.to_string())?,
        );
        let schedule = Arc::new(PlaybackSchedule::from_doc(&doc));
        let config = EngineConfig::new(sample_rate, max_block_size);
        let plugin_factory: Arc<dyn DeviceFactory> = Arc::new(PluginDeviceFactory::with_monitors(
            new_plugin_host(),
            plugin_descriptors,
            plugin_states,
            monitors,
        ));
        let mut factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(StockDeviceFactory)];
        if let Some(extension_factory) = extension_factory {
            factories.push(extension_factory);
        }
        factories.push(plugin_factory);
        let factory: Arc<dyn DeviceFactory> = Arc::new(CompositeDeviceFactory::new(factories));
        Ok((config, factory, plan, schedule, assets))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn prepare_wire(self) -> Result<WireBuild, String> {
        let Self {
            doc,
            caps,
            registry,
            extension_factory: _,
            plugin_descriptors,
            plugin_states,
            assets,
            sample_rate,
            max_block_size,
            monitors: _,
            extension_runtime,
        } = self;
        let plan =
            notwig_core::dsp::compile(&doc, &registry, caps).map_err(|error| error.to_string())?;
        let schedule = PlaybackSchedule::from_doc(&doc);
        let (extension_root, extension_settings) = extension_runtime.map_or_else(
            || (None, BTreeMap::new()),
            |runtime| (Some(runtime.root), runtime.settings),
        );
        Ok(WireBuild {
            config: EngineConfig::new(sample_rate, max_block_size),
            plan,
            schedule,
            assets,
            plugin_descriptors,
            plugin_states,
            extension_root,
            extension_settings,
        })
    }

    pub(crate) fn build(self) -> Result<Engine, String> {
        let (config, factory, plan, schedule, assets) = self.parts()?;
        let prepared = Engine::prepare_runtime(
            config,
            Arc::clone(&factory),
            plan,
            schedule,
            Arc::new(assets),
        )
        .map_err(|err| err.to_string())?;
        Ok(Engine::from_prepared(
            config,
            factory,
            prepared,
            TransportRuntime::default(),
            false,
        ))
    }

    pub(crate) fn prepare(self) -> Result<Box<PreparedRuntime>, String> {
        let (config, factory, plan, schedule, assets) = self.parts()?;
        Engine::prepare_runtime(config, factory, plan, schedule, Arc::new(assets))
            .map_err(|err| err.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub(crate) struct EngineProcessBuild {
    receiver: Option<mpsc::Receiver<(u64, Result<WireBuild, String>)>>,
    queued_request: Option<EngineBuildRequest>,
    requests: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl EngineProcessBuild {
    pub(crate) fn request(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        self.requests = self.requests.saturating_add(1);
        tracing::info!(
            generation = self.requests,
            sample_rate = request.sample_rate,
            block_size = request.max_block_size,
            "Engine plan rebuild requested"
        );
        if self.receiver.is_some() {
            tracing::debug!(generation = self.requests, "Queueing engine plan rebuild");
            self.queued_request = Some(request);
            return Ok(());
        }
        self.spawn(request)
    }

    pub(crate) fn poll(&mut self) -> Option<Result<WireBuild, String>> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok((generation, outcome)) => {
                self.receiver = None;
                if generation < self.requests {
                    if let Some(request) = self.queued_request.take()
                        && let Err(error) = self.spawn(request)
                    {
                        return Some(Err(error));
                    }
                    return None;
                }
                Some(outcome)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                Some(Err("Engine build thread disconnected.".into()))
            }
        }
    }

    pub(crate) fn building(&self) -> bool {
        self.receiver.is_some() || self.queued_request.is_some()
    }

    fn spawn(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        let generation = self.requests;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("notwig-engine-build".into())
            .spawn(move || {
                let started = Instant::now();
                let outcome = request.prepare_wire();
                tracing::info!(
                    generation,
                    elapsed_ms = started.elapsed().as_millis(),
                    success = outcome.is_ok(),
                    "Engine plan build finished"
                );
                let _ = sender.send((generation, outcome));
            })
            .map_err(|error| format!("Could not start engine build: {error}"))?;
        self.receiver = Some(receiver);
        Ok(())
    }
}

type EngineBuildResult = Result<Box<PreparedRuntime>, String>;
#[cfg(not(target_arch = "wasm32"))]
type EngineBuildMessage = (u64, EngineBuildResult);

#[derive(Default)]
pub(crate) struct EngineBuild {
    #[cfg(not(target_arch = "wasm32"))]
    receiver: Option<mpsc::Receiver<EngineBuildMessage>>,
    #[cfg(not(target_arch = "wasm32"))]
    queued_request: Option<EngineBuildRequest>,
    #[cfg(target_arch = "wasm32")]
    pending: Option<EngineBuildResult>,
    requests: u64,
}

impl EngineBuild {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn request(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        self.requests = self.requests.saturating_add(1);
        tracing::info!(
            generation = self.requests,
            sample_rate = request.sample_rate,
            block_size = request.max_block_size,
            "Engine rebuild requested"
        );
        if self.receiver.is_some() {
            tracing::debug!(generation = self.requests, "Queueing engine rebuild");
            self.queued_request = Some(request);
            return Ok(());
        }
        self.spawn(request)
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn request(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        self.requests = self.requests.saturating_add(1);
        tracing::info!(
            generation = self.requests,
            sample_rate = request.sample_rate,
            block_size = request.max_block_size,
            "Rebuilding engine"
        );
        let started = crate::clock::Instant::now();
        let prepared = request.prepare();
        tracing::info!(
            generation = self.requests,
            elapsed_ms = started.elapsed().as_millis(),
            success = prepared.is_ok(),
            "Engine build finished"
        );
        self.pending = Some(prepared);
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn poll(&mut self) -> Option<Result<Box<PreparedRuntime>, String>> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok((generation, outcome)) => {
                self.receiver = None;
                if generation < self.requests {
                    if let Some(request) = self.queued_request.take()
                        && let Err(err) = self.spawn(request)
                    {
                        return Some(Err(err));
                    }
                    return None;
                }
                Some(outcome)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                Some(Err("Engine build thread disconnected.".into()))
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn poll(&mut self) -> Option<Result<Box<PreparedRuntime>, String>> {
        self.pending.take()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn building(&self) -> bool {
        self.receiver.is_some() || self.queued_request.is_some()
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn building(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn requests(&self) -> u64 {
        self.requests
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        let generation = self.requests;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("notwig-engine-build".into())
            .spawn(move || {
                let started = Instant::now();
                let outcome = request.prepare();
                tracing::info!(
                    generation,
                    elapsed_ms = started.elapsed().as_millis(),
                    success = outcome.is_ok(),
                    "Engine build finished"
                );
                let _ = sender.send((generation, outcome));
            })
            .map_err(|err| format!("Could not start engine build: {err}"))?;
        self.receiver = Some(receiver);
        Ok(())
    }
}
