use std::sync::atomic::{AtomicU32, Ordering};

use js_sys::{BigInt, Function, Object, Reflect, Uint8Array};
use notwig_core::ParamValue;
use notwig_ext_api::{
    ABI_VERSION, AbiStatus, DeviceDrawContext, MAX_OUTPUT_BYTES, MAX_PANEL_FRAME_BYTES, PanelFrame,
    unpack_ptr_len,
};
use wasm_bindgen::{JsCast, JsValue};

use crate::error::ExtensionError;

const MAX_INPUT_BYTES: usize = 1024 * 1024;
static INSTANCE_COUNTER: AtomicU32 = AtomicU32::new(0);

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
    host_global: String,
}

impl Drop for WebExtensionInstance {
    fn drop(&mut self) {
        let global = js_sys::global();
        let _ = Reflect::delete_property(&global, &JsValue::from_str(&self.host_global));
    }
}

impl WebExtensionInstance {
    pub(crate) fn instantiate(
        module: &js_sys::WebAssembly::Module,
        extension_id: &str,
    ) -> Result<Self, ExtensionError> {
        let host_name = format!(
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
        Reflect::set(&global, &JsValue::from_str(&host_name), host.as_ref()).map_err(js_error)?;

        let log = Function::new_with_args(
            "level, ptr, len",
            &format!(
                "const s=globalThis['{host_name}'];if(s.mem){{const m=new Uint8Array(s.mem.buffer);const t=new TextDecoder().decode(m.subarray(ptr,ptr+len));(level===0?console.debug:level===1?console.info:console.warn)('['+s.id+'] '+t);}}"
            ),
        );
        let notify = Function::new_with_args(
            "ptr, len",
            &format!(
                "const s=globalThis['{host_name}'];if(s.mem){{const m=new Uint8Array(s.mem.buffer);const t=new TextDecoder().decode(m.subarray(ptr,ptr+len));console.info('['+s.id+'] '+t);}}"
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
        let instance = js_sys::WebAssembly::Instance::new(module, &imports).map_err(js_error)?;
        let exports = instance.exports();
        let memory: js_sys::WebAssembly::Memory =
            Reflect::get(&exports, &JsValue::from_str("memory"))
                .map_err(js_error)?
                .dyn_into()
                .map_err(|_| ExtensionError::MissingExport("memory".into()))?;
        Reflect::set(&host, &JsValue::from_str("mem"), memory.as_ref()).map_err(js_error)?;

        let abi_version = function(&exports, "notwig_abi_version")?;
        let version = call_i32(&abi_version, &[])?;
        let expected = i32::try_from(ABI_VERSION)
            .map_err(|_| ExtensionError::Abi("ABI version does not fit in i32".into()))?;
        if version != expected {
            return Err(ExtensionError::Abi(format!(
                "extension ABI {version} does not match host {ABI_VERSION}"
            )));
        }

        Ok(Self {
            _instance: instance,
            alloc: function(&exports, "notwig_alloc")?,
            free: function(&exports, "notwig_free")?,
            describe: function(&exports, "notwig_device_describe")?,
            create: function(&exports, "notwig_device_create")?,
            destroy: function(&exports, "notwig_device_destroy")?,
            prepare: function(&exports, "notwig_device_prepare")?,
            set_param: function(&exports, "notwig_device_set_param")?,
            process: function(&exports, "notwig_device_process")?,
            latency: function(&exports, "notwig_device_latency")?,
            draw: function(&exports, "notwig_device_draw")?,
            memory,
            host_global: host_name,
        })
    }

    pub(crate) fn describe(
        &mut self,
        type_id: &str,
    ) -> Result<notwig_core::dsp::DeviceSpec, ExtensionError> {
        let (ptr, len) = self.write_input(type_id.as_bytes())?;
        let packed = call_u64(&self.describe, &[JsValue::from(ptr), JsValue::from(len)])?;
        self.free_input(ptr, len);
        let json = self.read_packed_json(packed)?;
        serde_json::from_slice(&json)
            .map_err(|error| ExtensionError::Abi(format!("parse device spec: {error}")))
    }

    pub(crate) fn create_device(&mut self, type_id: &str) -> Result<i32, ExtensionError> {
        let (ptr, len) = self.write_input(type_id.as_bytes())?;
        let handle = call_i32(&self.create, &[JsValue::from(ptr), JsValue::from(len)])?;
        self.free_input(ptr, len);
        if handle < 0 {
            return Err(ExtensionError::InvalidHandle);
        }
        Ok(handle)
    }

    pub(crate) fn destroy_device(&mut self, handle: i32) {
        let _ = self
            .destroy
            .call1(&JsValue::UNDEFINED, &JsValue::from(handle));
    }

    pub(crate) fn prepare_device(
        &mut self,
        handle: i32,
        sample_rate: u32,
        max_block: usize,
        channels: u16,
    ) -> Result<(), ExtensionError> {
        let sample_rate = i32::try_from(sample_rate)
            .map_err(|_| ExtensionError::Abi("sample rate does not fit in i32".into()))?;
        let max_block = i32::try_from(max_block)
            .map_err(|_| ExtensionError::Abi("block size does not fit in i32".into()))?;
        let status = call_i32(
            &self.prepare,
            &[
                JsValue::from(handle),
                JsValue::from(sample_rate),
                JsValue::from(max_block),
                JsValue::from(i32::from(channels)),
            ],
        )?;
        check_status(status)
    }

    pub(crate) fn set_param(
        &mut self,
        handle: i32,
        param_id: notwig_core::ParamId,
        value: &ParamValue,
    ) -> Result<(), ExtensionError> {
        let id = param_id.to_string();
        let (ptr, len) = self.write_input(id.as_bytes())?;
        let (tag, raw) = param_wire(value);
        let status = call_i32(
            &self.set_param,
            &[
                JsValue::from(handle),
                JsValue::from(ptr),
                JsValue::from(len),
                JsValue::from(tag),
                JsValue::from(raw),
            ],
        )?;
        self.free_input(ptr, len);
        check_status(status)
    }

    pub(crate) fn process(
        &mut self,
        handle: i32,
        audio_ptr: i32,
        frames: usize,
        events_ptr: i32,
        event_count: usize,
        sample_position: u64,
        playing: bool,
    ) -> Result<i32, ExtensionError> {
        let frames = i32::try_from(frames)
            .map_err(|_| ExtensionError::Abi("frame count does not fit in i32".into()))?;
        let event_count = i32::try_from(event_count)
            .map_err(|_| ExtensionError::Abi("event count does not fit in i32".into()))?;
        let sample_position = i64::try_from(sample_position)
            .map_err(|_| ExtensionError::Abi("sample position does not fit in i64".into()))?;
        self.process
            .call7(
                &JsValue::UNDEFINED,
                &JsValue::from(handle),
                &JsValue::from(audio_ptr),
                &JsValue::from(frames),
                &JsValue::from(events_ptr),
                &JsValue::from(event_count),
                &JsValue::from(sample_position),
                &JsValue::from(i32::from(playing)),
            )
            .map_err(js_error)?
            .as_f64()
            .map(|value| value as i32)
            .ok_or_else(|| ExtensionError::Abi("process returned a non-number".into()))
    }

    pub(crate) fn latency(&mut self, handle: i32) -> u32 {
        self.latency
            .call1(&JsValue::UNDEFINED, &JsValue::from(handle))
            .ok()
            .and_then(|value| value.as_f64())
            .and_then(|value| u32::try_from(value as i64).ok())
            .unwrap_or(0)
    }

    pub(crate) fn draw_panel(
        &mut self,
        handle: i32,
        context: &DeviceDrawContext,
    ) -> Result<PanelFrame, ExtensionError> {
        let context = postcard::to_allocvec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode egui context: {error}")))?;
        let (ptr, len) = self.write_input(&context)?;
        let packed = call_u64(
            &self.draw,
            &[
                JsValue::from(handle),
                JsValue::from(ptr),
                JsValue::from(len),
            ],
        )?;
        self.free_input(ptr, len);
        let bytes = self.read_packed_bytes(packed, MAX_PANEL_FRAME_BYTES)?;
        let frame: PanelFrame = postcard::from_bytes(&bytes)
            .map_err(|error| ExtensionError::Abi(format!("decode egui frame: {error}")))?;
        frame
            .validate()
            .map_err(|error| ExtensionError::Abi(format!("invalid egui frame: {error}")))?;
        Ok(frame)
    }

    pub(crate) fn alloc_audio(&mut self, bytes: usize) -> Result<i32, ExtensionError> {
        let len = i32::try_from(bytes)
            .map_err(|_| ExtensionError::Abi("audio buffer is too large".into()))?;
        let ptr = call_i32(&self.alloc, &[JsValue::from(len)])?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        Ok(ptr)
    }

    pub(crate) fn write_bytes(&mut self, ptr: i32, bytes: &[u8]) -> Result<(), ExtensionError> {
        let start =
            u32::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let end = start
            .checked_add(
                u32::try_from(bytes.len())
                    .map_err(|_| ExtensionError::Abi("buffer is too large".into()))?,
            )
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        let view = Uint8Array::new(&self.memory.buffer());
        if end > view.length() {
            return Err(ExtensionError::Abi("out-of-bounds write".into()));
        }
        view.set(&Uint8Array::from(bytes), start);
        Ok(())
    }

    pub(crate) fn read_into(&self, ptr: i32, out: &mut [u8]) -> Result<(), ExtensionError> {
        let start =
            u32::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let end = start
            .checked_add(
                u32::try_from(out.len())
                    .map_err(|_| ExtensionError::Abi("buffer is too large".into()))?,
            )
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        let view = Uint8Array::new(&self.memory.buffer());
        if end > view.length() {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        out.copy_from_slice(&view.slice(start, end).to_vec());
        Ok(())
    }

    fn read_bytes(&self, ptr: i32, len: i32) -> Result<Vec<u8>, ExtensionError> {
        let start =
            u32::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let len = u32::try_from(len).map_err(|_| ExtensionError::Abi("negative length".into()))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        let view = Uint8Array::new(&self.memory.buffer());
        if end > view.length() {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        Ok(view.slice(start, end).to_vec())
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<(i32, i32), ExtensionError> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(ExtensionError::Abi("input is too large".into()));
        }
        let len = i32::try_from(bytes.len())
            .map_err(|_| ExtensionError::Abi("input is too large".into()))?;
        let ptr = call_i32(&self.alloc, &[JsValue::from(len)])?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        self.write_bytes(ptr, bytes)?;
        Ok((ptr, len))
    }

    fn free_input(&mut self, ptr: i32, len: i32) {
        let _ = self.free.call2(
            &JsValue::UNDEFINED,
            &JsValue::from(ptr),
            &JsValue::from(len),
        );
    }

    fn read_packed_json(&mut self, packed: u64) -> Result<Vec<u8>, ExtensionError> {
        self.read_packed_bytes(packed, MAX_OUTPUT_BYTES)
    }

    fn read_packed_bytes(&mut self, packed: u64, max: usize) -> Result<Vec<u8>, ExtensionError> {
        let (ptr, len) = unpack_ptr_len(packed);
        let ptr = i32::try_from(ptr)
            .map_err(|_| ExtensionError::Abi("negative result pointer".into()))?;
        let len =
            i32::try_from(len).map_err(|_| ExtensionError::Abi("negative result length".into()))?;
        if len == 0 {
            return Err(ExtensionError::Abi("empty result".into()));
        }
        if usize::try_from(len).is_ok_and(|len| len > max) {
            self.free_input(ptr, len);
            return Err(ExtensionError::Abi("result is too large".into()));
        }
        let bytes = self.read_bytes(ptr, len)?;
        self.free_input(ptr, len);
        Ok(bytes)
    }
}

fn function(exports: &Object, name: &str) -> Result<Function, ExtensionError> {
    Reflect::get(exports, &JsValue::from_str(name))
        .map_err(|_| ExtensionError::MissingExport(name.to_owned()))?
        .dyn_into()
        .map_err(|_| ExtensionError::MissingExport(name.to_owned()))
}

fn call_i32(function: &Function, args: &[JsValue]) -> Result<i32, ExtensionError> {
    call_value(function, args)?
        .as_f64()
        .map(|value| value as i32)
        .ok_or_else(|| ExtensionError::Abi("extension returned a non-number".into()))
}

fn call_u64(function: &Function, args: &[JsValue]) -> Result<u64, ExtensionError> {
    let value = call_value(function, args)?;
    let bigint = BigInt::new(&value).map_err(|error| ExtensionError::Abi(format!("{error:?}")))?;
    u64::try_from(bigint)
        .map_err(|_| ExtensionError::Abi("extension returned an invalid u64".into()))
}

fn call_value(function: &Function, args: &[JsValue]) -> Result<JsValue, ExtensionError> {
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    function
        .apply(&JsValue::UNDEFINED, &array)
        .map_err(js_error)
}

fn param_wire(value: &ParamValue) -> (i32, f64) {
    match value {
        ParamValue::F64(value) => (0, *value),
        ParamValue::I64(value) => (1, *value as f64),
        ParamValue::Bool(value) => (2, f64::from(u8::from(*value))),
        ParamValue::EnumIndex(value) => (3, f64::from(*value)),
    }
}

fn check_status(status: i32) -> Result<(), ExtensionError> {
    match AbiStatus::from_i32(status) {
        Some(AbiStatus::Ok) => Ok(()),
        Some(AbiStatus::FuelExhausted) => Err(ExtensionError::BudgetExceeded),
        Some(other) => Err(ExtensionError::Abi(format!("extension returned {other:?}"))),
        None => Err(ExtensionError::Abi(format!(
            "extension returned unknown status {status}"
        ))),
    }
}

fn js_error(value: JsValue) -> ExtensionError {
    ExtensionError::Wasm(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
