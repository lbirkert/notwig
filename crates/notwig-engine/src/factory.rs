use std::collections::BTreeMap;
use std::sync::Mutex;

use notwig_core::{ParamId, ParamValue};
use notwig_dsp::{
    CompiledPlan, DeviceProcessor, Latency, NoteEvent, PlanDevice, ProcessContext, ProcessStatus,
    RenderConfig,
};
use notwig_plugins::{PluginDescriptor, PluginHost, PluginInstance};

pub trait DeviceFactory: Send + Sync {
    fn create(&self, plan: &CompiledPlan, device: &PlanDevice) -> Option<Box<dyn DeviceProcessor>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StockDeviceFactory;

impl DeviceFactory for StockDeviceFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        notwig_dsp::instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

pub struct PluginDeviceFactory {
    host: Mutex<PluginHost>,
    descriptors: BTreeMap<String, PluginDescriptor>,
}

impl std::fmt::Debug for PluginDeviceFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginDeviceFactory")
    }
}

impl PluginDeviceFactory {
    #[must_use]
    pub fn new(host: PluginHost, descriptors: Vec<PluginDescriptor>) -> Self {
        Self {
            host: Mutex::new(host),
            descriptors: descriptors
                .into_iter()
                .map(|descriptor| (descriptor.type_id.clone(), descriptor))
                .collect(),
        }
    }
}

struct PluginDevice(Box<dyn PluginInstance>);

impl DeviceProcessor for PluginDevice {
    fn prepare(&mut self, config: RenderConfig) -> Result<(), notwig_dsp::GraphError> {
        self.0.prepare(config)
    }

    fn reset(&mut self) {
        self.0.reset();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        self.0.set_param(param_id, value);
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        self.0.process(ctx, audio, events)
    }

    fn latency(&self) -> Latency {
        self.0.latency()
    }
}

impl DeviceFactory for PluginDeviceFactory {
    fn create(&self, plan: &CompiledPlan, device: &PlanDevice) -> Option<Box<dyn DeviceProcessor>> {
        if let Some(processor) = StockDeviceFactory.create(plan, device) {
            return Some(processor);
        }
        let mut host = self.host.lock().ok()?;
        let descriptor = self.descriptors.get(&device.type_id)?;
        host.load(descriptor)
            .ok()
            .map(|instance| Box::new(PluginDevice(instance)) as Box<dyn DeviceProcessor>)
    }
}
