use crate::{DeviceId, ParamId, ParamValue};

use super::devices::{
    ContainerRouting, DeviceProcessor, DeviceSpec, Latency, ProcessContext, ProcessStatus,
};
use super::graph::GraphError;
use super::smoothing::SmoothedValue;

/// Host-owned processor for a [`crate::DeviceKind::Container`] device.
///
/// The container owns an optional stage (the device implementation, such as an
/// extension processor) and a serial child chain. The host controls the signal
/// path, nested parameter routing, latency, and the optional dry/wet blend, so
/// extension containers do not need any child-processor ABI.
pub struct ContainerProcessor {
    routing: ContainerRouting,
    wet_param: Option<ParamId>,
    wet: SmoothedValue,
    sample_rate: f64,
    dry: Vec<f32>,
    stage: Option<Box<dyn DeviceProcessor>>,
    children: Vec<(DeviceId, Box<dyn DeviceProcessor>)>,
}

impl ContainerProcessor {
    /// Creates a container from a device spec.
    ///
    /// `spec.container` supplies the routing and optional wet parameter. A wet
    /// parameter that is not declared in `spec.params` is ignored.
    #[must_use]
    pub fn new(spec: &DeviceSpec) -> Self {
        let container = spec.container.clone().unwrap_or_default();
        let wet_param = container
            .wet_param
            .filter(|id| spec.params.iter().any(|param| param.id == *id));
        let default_wet = wet_param
            .and_then(|id| spec.params.iter().find(|param| param.id == id))
            .map_or(1.0, |param| {
                param.default.as_f64().unwrap_or(1.0).clamp(0.0, 1.0)
            });
        let sample_rate = 48_000.0;
        Self {
            routing: container.routing,
            wet_param,
            wet: SmoothedValue::new(default_wet, sample_rate, 15.0),
            sample_rate,
            dry: Vec::new(),
            stage: None,
            children: Vec::new(),
        }
    }

    /// Sets the device stage that runs inside the container.
    pub fn set_stage(&mut self, stage: Box<dyn DeviceProcessor>) {
        self.stage = Some(stage);
    }

    /// Appends one child processor to the container's serial child chain.
    pub fn add_child(&mut self, device_id: DeviceId, processor: Box<dyn DeviceProcessor>) {
        self.children.push((device_id, processor));
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.wet.set_sample_rate(self.sample_rate, 15.0);
    }

    fn has_wet_path(&self) -> bool {
        self.wet_param.is_some() && self.stage.is_some()
    }
}

impl DeviceProcessor for ContainerProcessor {
    fn prepare(&mut self, config: super::devices::RenderConfig) -> Result<(), GraphError> {
        self.set_sample_rate(f64::from(config.sample_rate));
        self.wet.snap();
        self.dry.resize(config.max_block_size.max(1) * 2, 0.0);
        if let Some(stage) = &mut self.stage {
            stage.prepare(config)?;
        }
        for (_, child) in &mut self.children {
            child.prepare(config)?;
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.wet.snap();
        self.dry.fill(0.0);
        if let Some(stage) = &mut self.stage {
            stage.reset();
        }
        for (_, child) in &mut self.children {
            child.reset();
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        if let Some(stage) = &mut self.stage {
            stage.set_param(param_id, value);
        }
        if Some(param_id) == self.wet_param {
            self.wet
                .set_target(value.as_f64().unwrap_or(1.0).clamp(0.0, 1.0));
        }
    }

    fn set_nested_param(
        &mut self,
        device_id: DeviceId,
        param_id: ParamId,
        value: &ParamValue,
    ) -> bool {
        for (child_id, child) in &mut self.children {
            if *child_id == device_id {
                child.set_param(param_id, value);
                return true;
            }
            if child.set_nested_param(device_id, param_id, value) {
                return true;
            }
        }
        false
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[super::devices::NoteEvent],
    ) -> ProcessStatus {
        let use_wet = self.has_wet_path();
        if use_wet {
            if self.dry.len() < audio.len() {
                self.dry.resize(audio.len(), 0.0);
            }
            self.dry[..audio.len()].copy_from_slice(audio);
        }

        let mut overloaded = false;
        match self.routing {
            ContainerRouting::StageThenChildren => {
                if let Some(stage) = &mut self.stage {
                    overloaded |= stage.process(ctx, audio, events).overloaded;
                }
                for (_, child) in &mut self.children {
                    overloaded |= child.process(ctx, audio, events).overloaded;
                }
            }
            ContainerRouting::ChildrenThenStage => {
                for (_, child) in &mut self.children {
                    overloaded |= child.process(ctx, audio, events).overloaded;
                }
                if let Some(stage) = &mut self.stage {
                    overloaded |= stage.process(ctx, audio, events).overloaded;
                }
            }
        }

        if use_wet {
            for (frame, dry) in audio.chunks_exact_mut(2).zip(self.dry.chunks_exact(2)) {
                let wet = self.wet.next() as f32;
                for channel in 0..2 {
                    frame[channel] = dry[channel] + (frame[channel] - dry[channel]) * wet;
                }
            }
        }

        ProcessStatus {
            overloaded,
            produced_silence: false,
        }
    }

    fn latency(&self) -> Latency {
        let stage = self
            .stage
            .as_ref()
            .map_or(0, |stage| stage.latency().samples);
        let children = self
            .children
            .iter()
            .map(|(_, child)| child.latency().samples)
            .fold(0u32, u32::saturating_add);
        Latency {
            samples: stage.saturating_add(children),
        }
    }
}
