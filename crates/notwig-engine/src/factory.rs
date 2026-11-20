use std::collections::BTreeMap;
use std::sync::Mutex;

use notwig_core::{DeviceId, ParamId, ParamValue};
use notwig_dsp::{
    AudioMonitor, CompiledPlan, DeviceFactory, DeviceProcessor, Latency, MonitorRegistry,
    NoteEvent, PlanDevice, ProcessContext, ProcessStatus, RenderConfig, StockDeviceFactory,
};
use notwig_plugins::{PluginDescriptor, PluginHost, PluginInstance, PluginState};

pub struct PluginDeviceFactory {
    host: Mutex<PluginHost>,
    descriptors: BTreeMap<String, PluginDescriptor>,
    states: BTreeMap<DeviceId, PluginState>,
    monitors: Option<std::sync::Arc<MonitorRegistry>>,
}

impl std::fmt::Debug for PluginDeviceFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginDeviceFactory")
    }
}

impl PluginDeviceFactory {
    #[must_use]
    pub fn new(
        host: PluginHost,
        descriptors: Vec<PluginDescriptor>,
        states: BTreeMap<DeviceId, PluginState>,
    ) -> Self {
        Self {
            host: Mutex::new(host),
            descriptors: descriptors
                .into_iter()
                .map(|descriptor| (descriptor.type_id.clone(), descriptor))
                .collect(),
            states,
            monitors: None,
        }
    }

    /// Build a factory that taps each plugin device's input for open editors.
    #[must_use]
    pub fn with_monitors(
        host: PluginHost,
        descriptors: Vec<PluginDescriptor>,
        states: BTreeMap<DeviceId, PluginState>,
        monitors: std::sync::Arc<MonitorRegistry>,
    ) -> Self {
        Self {
            host: Mutex::new(host),
            descriptors: descriptors
                .into_iter()
                .map(|descriptor| (descriptor.type_id.clone(), descriptor))
                .collect(),
            states,
            monitors: Some(monitors),
        }
    }
}

struct PluginDevice {
    instance: Box<dyn PluginInstance>,
    monitor: Option<std::sync::Arc<AudioMonitor>>,
}

impl DeviceProcessor for PluginDevice {
    fn prepare(&mut self, config: RenderConfig) -> Result<(), notwig_dsp::GraphError> {
        self.instance.prepare(config)
    }

    fn reset(&mut self) {
        self.instance.reset();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        self.instance.set_param(param_id, value);
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        if let Some(monitor) = &self.monitor {
            monitor.push_interleaved(audio, audio.len() / 2);
        }
        self.instance.process(ctx, audio, events)
    }

    fn latency(&self) -> Latency {
        self.instance.latency()
    }
}

impl DeviceFactory for PluginDeviceFactory {
    fn create(&self, plan: &CompiledPlan, device: &PlanDevice) -> Option<Box<dyn DeviceProcessor>> {
        if let Some(processor) = StockDeviceFactory.create(plan, device) {
            return Some(processor);
        }
        let mut host = self.host.lock().ok()?;
        let descriptor = self.descriptors.get(&device.type_id)?;
        let mut instance = host.load(descriptor).ok()?;
        if let Some(state) = self.states.get(&device.id) {
            let _ = host.restore_instance_state(instance.as_mut(), state);
        }
        let monitor = self
            .monitors
            .as_ref()
            .map(|monitors| monitors.monitor(device.id));
        Some(Box::new(PluginDevice { instance, monitor }) as Box<dyn DeviceProcessor>)
    }
}
