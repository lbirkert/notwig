use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

use notwig_core::protocol::{EngineMsg, PlanHandle, PlaybackSchedule};
use notwig_core::{AssetId, AudioAssetBuffer, ProjectDoc};
use notwig_dsp::EngineCaps;
use notwig_engine::{Engine, EngineConfig, PluginDeviceFactory};
use notwig_plugins::PluginDescriptor;

use crate::plugins::new_plugin_host;

pub(crate) struct EngineBuildRequest {
    pub(crate) doc: ProjectDoc,
    pub(crate) caps: EngineCaps,
    pub(crate) registry: notwig_dsp::DeviceRegistry,
    pub(crate) plugin_descriptors: Vec<PluginDescriptor>,
    pub(crate) assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub(crate) sample_rate: u32,
    pub(crate) max_block_size: usize,
}

impl EngineBuildRequest {
    pub(crate) fn build(self) -> Result<Engine, String> {
        let Self {
            doc,
            caps,
            registry,
            plugin_descriptors,
            assets,
            sample_rate,
            max_block_size,
        } = self;
        let plan = notwig_dsp::compile(&doc, &registry, caps).map_err(|err| err.to_string())?;
        let config = EngineConfig::new(sample_rate, max_block_size);
        let factory = Arc::new(PluginDeviceFactory::new(
            new_plugin_host(),
            plugin_descriptors,
        ));
        let mut engine = Engine::with_factory(config, factory);
        engine
            .apply(EngineMsg::UpdateSchedule {
                schedule: Arc::new(PlaybackSchedule::from_doc(&doc)),
            })
            .map_err(|err| err.to_string())?;
        engine
            .apply(EngineMsg::ApplyPlan {
                plan: PlanHandle::new(plan),
            })
            .map_err(|err| err.to_string())?;
        for (asset_id, buffer) in &assets {
            engine
                .apply(EngineMsg::PrepareAudioAsset {
                    asset_id: *asset_id,
                    buffer: Arc::clone(buffer),
                })
                .map_err(|err| err.to_string())?;
        }
        Ok(engine)
    }
}

#[derive(Default)]
pub(crate) struct EngineBuild {
    receiver: Option<mpsc::Receiver<Result<Engine, String>>>,
    queued: bool,
    requests: u64,
}

impl EngineBuild {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn request(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        self.requests = self.requests.saturating_add(1);
        if self.receiver.is_some() {
            self.queued = true;
            return Ok(());
        }
        self.spawn(request)
    }

    pub(crate) fn poll(&mut self) -> Option<Result<Engine, String>> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok(outcome) => {
                self.receiver = None;
                Some(outcome)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                Some(Err("Engine build thread disconnected.".into()))
            }
        }
    }

    pub(crate) fn start_queued(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        if !self.queued {
            return Ok(());
        }
        self.queued = false;
        self.spawn(request)
    }

    pub(crate) fn queued(&self) -> bool {
        self.queued
    }

    pub(crate) fn building(&self) -> bool {
        self.receiver.is_some() || self.queued
    }

    pub(crate) fn requests(&self) -> u64 {
        self.requests
    }

    fn spawn(&mut self, request: EngineBuildRequest) -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("notwig-engine-build".into())
            .spawn(move || {
                let _ = sender.send(request.build());
            })
            .map_err(|err| format!("Could not start engine build: {err}"))?;
        self.receiver = Some(receiver);
        Ok(())
    }
}
