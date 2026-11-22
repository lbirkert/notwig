use std::sync::Arc;

use super::devices::DeviceProcessor;
use super::graph::{CompiledPlan, PlanDevice};

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
        super::stock::instantiate_stock_params(&device.type_id, &device.spec, &device.param_values)
    }
}

#[derive(Clone, Default)]
pub struct CompositeDeviceFactory {
    factories: Vec<Arc<dyn DeviceFactory>>,
}

impl std::fmt::Debug for CompositeDeviceFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeDeviceFactory")
            .field("factories", &self.factories.len())
            .finish()
    }
}

impl CompositeDeviceFactory {
    #[must_use]
    pub fn new(factories: Vec<Arc<dyn DeviceFactory>>) -> Self {
        Self { factories }
    }

    pub fn push(&mut self, factory: Arc<dyn DeviceFactory>) {
        self.factories.push(factory);
    }
}

impl DeviceFactory for CompositeDeviceFactory {
    fn create(&self, plan: &CompiledPlan, device: &PlanDevice) -> Option<Box<dyn DeviceProcessor>> {
        self.factories
            .iter()
            .find_map(|factory| factory.create(plan, device))
    }
}
