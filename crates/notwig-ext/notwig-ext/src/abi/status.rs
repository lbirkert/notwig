//! ABI status decoding and parameter wire conversion.

use notwig_core::ParamValue;
use notwig_ext_api::AbiStatus;

use crate::error::ExtensionError;

pub(super) fn param_wire(value: &ParamValue) -> (i32, f64) {
    match value {
        ParamValue::F64(value) => (0, *value),
        ParamValue::I64(value) => (1, *value as f64),
        ParamValue::Bool(value) => (2, f64::from(u8::from(*value))),
        ParamValue::EnumIndex(value) => (3, f64::from(*value)),
    }
}

pub(super) fn check_status(status: i32) -> Result<(), ExtensionError> {
    match AbiStatus::from_i32(status) {
        Some(AbiStatus::Ok) => Ok(()),
        Some(AbiStatus::FuelExhausted) => Err(ExtensionError::BudgetExceeded),
        Some(other) => Err(ExtensionError::Abi(format!("extension returned {other:?}"))),
        None => Err(ExtensionError::Abi(format!(
            "extension returned unknown status {status}"
        ))),
    }
}
