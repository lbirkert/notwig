//! Exported ABI calls for the browser WASM instance.

use js_sys::{BigInt, Function, Object, Reflect};
use notwig_core::ParamValue;
use notwig_ext_api::{DeviceDrawContext, MAX_PANEL_FRAME_BYTES, PanelFrame};
use wasm_bindgen::{JsCast, JsValue};

use super::WebExtensionInstance;
use super::status::{check_status, param_wire};
use crate::error::ExtensionError;

impl WebExtensionInstance {
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
            .map_err(super::status::js_error)?
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
}

pub(super) fn export_function(exports: &Object, name: &str) -> Result<Function, ExtensionError> {
    Reflect::get(exports, &JsValue::from_str(name))
        .map_err(|_| ExtensionError::MissingExport(name.to_owned()))?
        .dyn_into()
        .map_err(|_| ExtensionError::MissingExport(name.to_owned()))
}

pub(super) fn call_i32(function: &Function, args: &[JsValue]) -> Result<i32, ExtensionError> {
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
        .map_err(super::status::js_error)
}
