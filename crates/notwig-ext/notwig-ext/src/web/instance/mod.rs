//! Browser WASM extension instance.
//!
//! [`mod@host`] builds the JS import object, [`memory`] owns typed-array access
//! to the guest buffer, [`abi`] performs exported calls, and [`status`] decodes
//! statuses and parameter wire values.

use js_sys::{Function, Reflect};
use notwig_ext_api::ABI_VERSION;
use wasm_bindgen::{JsCast, JsValue};

use crate::error::ExtensionError;

mod abi;
mod host;
mod memory;
mod status;

use host::HostGlue;
use status::js_error;

pub(crate) struct WebExtensionInstance {
    _instance: js_sys::WebAssembly::Instance,
    memory: js_sys::WebAssembly::Memory,
    alloc: Function,
    free: Function,
    describe: Function,
    create: Function,
    destroy: Function,
    prepare: Function,
    set_param: Function,
    process: Function,
    latency: Function,
    draw: Function,
    sub_editor_create: Option<Function>,
    sub_editor_destroy: Option<Function>,
    sub_editor_draw: Option<Function>,
    command: Option<Function>,
    preload: Option<Function>,
    load: Option<Function>,
    unload: Option<Function>,
    registrations: Option<Function>,
    host_global: String,
}

impl Drop for WebExtensionInstance {
    fn drop(&mut self) {
        self.run_unload();
        let global = js_sys::global();
        let _ = Reflect::delete_property(&global, &JsValue::from_str(&self.host_global));
    }
}

impl WebExtensionInstance {
    pub(crate) fn instantiate(
        module: &js_sys::WebAssembly::Module,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        let mut instance = Self::create(module, extension_id)?;
        instance.preload_phase()?;
        instance.load_phase()?;
        Ok(instance)
    }

    pub(crate) fn instantiate_preload(
        module: &js_sys::WebAssembly::Module,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        let mut instance = Self::create(module, extension_id)?;
        instance.preload_phase()?;
        Ok(instance)
    }

    fn create(
        module: &js_sys::WebAssembly::Module,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        let (glue, imports) = HostGlue::new(extension_id)?;
        let instance = js_sys::WebAssembly::Instance::new(module, &imports).map_err(js_error)?;
        let exports = instance.exports();
        let memory: js_sys::WebAssembly::Memory =
            Reflect::get(&exports, &JsValue::from_str("memory"))
                .map_err(js_error)?
                .dyn_into()
                .map_err(|_| ExtensionError::MissingExport("memory".into()))?;
        glue.bind_memory(&memory)?;

        let abi_version = abi::export_function(&exports, "notwig_abi_version")?;
        let version = abi::call_i32(&abi_version, &[])?;
        let expected = i32::try_from(ABI_VERSION)
            .map_err(|_| ExtensionError::Abi("ABI version does not fit in i32".into()))?;
        if version != expected {
            return Err(ExtensionError::Abi(format!(
                "extension ABI {version} does not match host {ABI_VERSION}"
            )));
        }

        let runtime = Self {
            _instance: instance,
            alloc: abi::export_function(&exports, "notwig_alloc")?,
            free: abi::export_function(&exports, "notwig_free")?,
            describe: abi::export_function(&exports, "notwig_device_describe")?,
            create: abi::export_function(&exports, "notwig_device_create")?,
            destroy: abi::export_function(&exports, "notwig_device_destroy")?,
            prepare: abi::export_function(&exports, "notwig_device_prepare")?,
            set_param: abi::export_function(&exports, "notwig_device_set_param")?,
            process: abi::export_function(&exports, "notwig_device_process")?,
            latency: abi::export_function(&exports, "notwig_device_latency")?,
            draw: abi::export_function(&exports, "notwig_device_draw")?,
            sub_editor_create: Reflect::get(
                &exports,
                &JsValue::from_str("notwig_audio_sub_editor_create"),
            )
            .ok()
            .and_then(|value| value.dyn_into().ok()),
            sub_editor_destroy: Reflect::get(
                &exports,
                &JsValue::from_str("notwig_audio_sub_editor_destroy"),
            )
            .ok()
            .and_then(|value| value.dyn_into().ok()),
            sub_editor_draw: Reflect::get(
                &exports,
                &JsValue::from_str("notwig_audio_sub_editor_draw"),
            )
            .ok()
            .and_then(|value| value.dyn_into().ok()),
            command: Reflect::get(&exports, &JsValue::from_str("notwig_command_execute"))
                .ok()
                .and_then(|value| value.dyn_into().ok()),
            preload: Reflect::get(&exports, &JsValue::from_str("notwig_ext_preload"))
                .ok()
                .and_then(|value| value.dyn_into().ok()),
            load: Reflect::get(&exports, &JsValue::from_str("notwig_ext_load"))
                .ok()
                .and_then(|value| value.dyn_into().ok()),
            unload: Reflect::get(&exports, &JsValue::from_str("notwig_ext_unload"))
                .ok()
                .and_then(|value| value.dyn_into().ok()),
            registrations: Reflect::get(&exports, &JsValue::from_str("notwig_ext_registrations"))
                .ok()
                .and_then(|value| value.dyn_into().ok()),
            memory,
            host_global: glue.global_name,
        };
        Ok(runtime)
    }
}
