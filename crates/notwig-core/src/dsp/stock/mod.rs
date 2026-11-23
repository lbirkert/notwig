use std::collections::BTreeMap;

use crate::{DeviceInstance, ParamId, ParamValue};

use super::devices::{DeviceProcessor, DeviceSpec};

mod biquad;
mod chain;
mod eq;
mod specs;
mod tool;

pub use chain::ChainProcessor;
pub use eq::{EqProcessor, eq_magnitude_response};
pub use specs::{
    CHAIN_TYPE_ID, EQ_TYPE_ID, TOOL_TYPE_ID, band_param, chain_spec, eq_spec, p_dry_wet,
    p_eq_output_gain, p_eq_shift, p_input_gain, p_output_gain, p_pan, p_polarity_l, p_polarity_r,
    p_swap_lr, p_width, param_id, stock_registry, tool_spec,
};
pub use tool::ToolProcessor;

#[must_use]
pub fn instantiate_stock_params(
    type_id: &str,
    spec: &DeviceSpec,
    param_values: &BTreeMap<ParamId, ParamValue>,
) -> Option<Box<dyn DeviceProcessor>> {
    let mut processor: Box<dyn DeviceProcessor> = match type_id {
        TOOL_TYPE_ID => Box::new(ToolProcessor::new(spec)),
        EQ_TYPE_ID => Box::new(EqProcessor::new(spec)),
        CHAIN_TYPE_ID => Box::new(ChainProcessor::new(spec)),
        _ => return None,
    };
    for param in &spec.params {
        processor.set_param(param.id, &param.default);
    }
    for (id, value) in param_values {
        processor.set_param(*id, value);
    }
    Some(processor)
}

#[must_use]
pub fn instantiate_stock(
    instance: &DeviceInstance,
    spec: &DeviceSpec,
) -> Option<Box<dyn DeviceProcessor>> {
    let params: BTreeMap<ParamId, ParamValue> = instance
        .param_values
        .iter()
        .map(|(id, value)| (*id, value.clone()))
        .collect();
    instantiate_stock_params(&instance.type_id, spec, &params)
}
