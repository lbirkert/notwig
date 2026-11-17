use notwig_dsp::{CompiledPlan, DeviceProcessor, PlanDevice};

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
