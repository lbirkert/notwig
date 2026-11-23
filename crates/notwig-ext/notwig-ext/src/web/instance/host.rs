//! JavaScript host glue for browser WASM imports.

use std::sync::atomic::{AtomicU32, Ordering};

use js_sys::{Function, Object, Reflect};
use wasm_bindgen::JsValue;

use super::status::js_error;
use crate::error::ExtensionError;

/// Keep guest log/notify calls bounded in the browser glue.
///
/// The browser `WebAssembly` API provides no fuel metering or host-side linear
/// memory cap, so the only enforceable limit here is how much text one import
/// copies and decodes.
const MAX_HOST_LOG_BYTES: i32 = 8 * 1024;

static INSTANCE_COUNTER: AtomicU32 = AtomicU32::new(0);

pub(super) struct HostGlue {
    pub(super) global_name: String,
    pub(super) host: Object,
}

impl HostGlue {
    pub(super) fn new(extension_id: &str) -> Result<(Self, Object), ExtensionError> {
        let global_name = format!(
            "__notwig_extension_{}",
            INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let global = js_sys::global();
        let host = Object::new();
        Reflect::set(&host, &JsValue::from_str("mem"), &JsValue::NULL).map_err(js_error)?;
        Reflect::set(
            &host,
            &JsValue::from_str("id"),
            &JsValue::from_str(extension_id),
        )
        .map_err(js_error)?;
        Reflect::set(&global, &JsValue::from_str(&global_name), host.as_ref()).map_err(js_error)?;

        let log = Function::new_with_args(
            "level, ptr, len",
            &format!(
                "const s=globalThis['{global_name}'];if(s.mem){{const n=Math.min(Math.max(len,0),{MAX_HOST_LOG_BYTES});const m=new Uint8Array(s.mem.buffer);const t=new TextDecoder().decode(m.subarray(ptr,ptr+n))+(len>n?'…':'');(level===0?console.debug:level===1?console.info:console.warn)('['+s.id+'] '+t);}}"
            ),
        );
        let notify = Function::new_with_args(
            "ptr, len",
            &format!(
                "const s=globalThis['{global_name}'];if(s.mem){{const n=Math.min(Math.max(len,0),{MAX_HOST_LOG_BYTES});const m=new Uint8Array(s.mem.buffer);const t=new TextDecoder().decode(m.subarray(ptr,ptr+n))+(len>n?'…':'');console.info('['+s.id+'] '+t);}}"
            ),
        );

        let host_imports = Object::new();
        Reflect::set(&host_imports, &JsValue::from_str("log"), log.as_ref()).map_err(js_error)?;
        Reflect::set(&host_imports, &JsValue::from_str("notify"), notify.as_ref())
            .map_err(js_error)?;
        let imports = Object::new();
        Reflect::set(
            &imports,
            &JsValue::from_str("notwig.host"),
            host_imports.as_ref(),
        )
        .map_err(js_error)?;
        Ok((Self { global_name, host }, imports))
    }

    pub(super) fn bind_memory(
        &self,
        memory: &js_sys::WebAssembly::Memory,
    ) -> Result<(), ExtensionError> {
        Reflect::set(&self.host, &JsValue::from_str("mem"), memory.as_ref())
            .map(|_| ())
            .map_err(js_error)
    }
}
