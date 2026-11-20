use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;

use notwig_core::protocol::PlaybackSchedule;
use notwig_core::{AssetId, AudioAssetBuffer, DeviceId, ProjectDoc};
use notwig_dsp::{
    CompiledPlan, CompositeDeviceFactory, DeviceFactory, EngineCaps, StockDeviceFactory,
};
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory, PreparedRuntime, TransportRuntime};
use notwig_plugins::{PluginDescriptor, PluginState};

use crate::plugins::new_plugin_host;

pub(crate) struct EngineBuildRequest {
    pub(crate) doc: ProjectDoc,
    pub(crate) caps: EngineCaps,
    pub(crate) registry: notwig_dsp::DeviceRegistry,
    pub(crate) extension_factory: Option<Arc<dyn DeviceFactory>>,
    pub(crate) plugin_descriptors: Vec<PluginDescriptor>,
    pub(crate) plugin_states: BTreeMap<DeviceId, PluginState>,
    pub(crate) assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub(crate) sample_rate: u32,
    pub(crate) max_block_size: usize,
    pub(crate) monitors: Arc<notwig_dsp::MonitorRegistry>,
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
        } = self;
        let plan =
            Arc::new(notwig_dsp::compile(&doc, &registry, caps).map_err(|err| err.to_string())?);
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
        if self.receiver.is_some() {
            self.queued_request = Some(request);
            return Ok(());
        }
        self.spawn(request)
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn request(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        self.requests = self.requests.saturating_add(1);
        self.pending = Some(request.prepare());
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
                let _ = sender.send((generation, request.prepare()));
            })
            .map_err(|err| format!("Could not start engine build: {err}"))?;
        self.receiver = Some(receiver);
        Ok(())
    }
}
