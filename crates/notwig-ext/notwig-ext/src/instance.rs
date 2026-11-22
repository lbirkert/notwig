use std::sync::Arc;

use notwig_core::dsp::DeviceSpec;
use notwig_core::{ParamId, ParamValue};
use notwig_ext_api::{DeviceDrawContext, PanelFrame};

use crate::abi::{ExtensionEngines, ExtensionInstance as WasmInstance};
use crate::compiled::ExtensionRuntime;
use crate::error::ExtensionError;
use crate::native::NativeInstance;

/// A device instance backed by either a WASM module or a native library.
pub(crate) enum ExtensionInstance {
    Wasm(WasmInstance),
    Native(NativeInstance),
}

impl ExtensionInstance {
    pub(crate) fn instantiate(
        runtime: &ExtensionRuntime,
        engines: &ExtensionEngines,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        match runtime {
            ExtensionRuntime::Wasm(module) => Ok(Self::Wasm(WasmInstance::instantiate(
                engines.engine(),
                module,
                extension_id,
            )?)),
            ExtensionRuntime::Native(library) => {
                Ok(Self::Native(NativeInstance::new(Arc::clone(library))))
            }
        }
    }

    pub(crate) fn describe(&mut self, type_id: &str) -> Result<DeviceSpec, ExtensionError> {
        match self {
            Self::Wasm(instance) => instance.describe(type_id),
            Self::Native(instance) => instance.describe(type_id),
        }
    }

    pub(crate) fn create_device(&mut self, type_id: &str) -> Result<i32, ExtensionError> {
        match self {
            Self::Wasm(instance) => instance.create_device(type_id),
            Self::Native(instance) => instance.create_device(type_id),
        }
    }

    pub(crate) fn set_param(
        &mut self,
        handle: i32,
        param_id: ParamId,
        value: &ParamValue,
    ) -> Result<(), ExtensionError> {
        match self {
            Self::Wasm(instance) => instance.set_param(handle, param_id, value),
            Self::Native(instance) => instance.set_param(handle, param_id, value),
        }
    }

    pub(crate) fn draw_panel(
        &mut self,
        handle: i32,
        context: &DeviceDrawContext,
    ) -> Result<PanelFrame, ExtensionError> {
        match self {
            Self::Wasm(instance) => instance.draw_panel(handle, context),
            Self::Native(instance) => instance.draw_panel(handle, context),
        }
    }
}
