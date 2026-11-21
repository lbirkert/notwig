#![allow(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

use libloading::Library;
use notwig_core::ParamValue;
use notwig_ext_api::{ABI_VERSION, AbiStatus, DeviceDrawContext, MAX_OUTPUT_BYTES, PanelDraw};

use crate::error::ExtensionError;

/// Matches `NotwigNativeBytes` in the generated native extension ABI.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeBytes {
    ptr: *const u8,
    len: usize,
}

type AbiVersionFn = unsafe extern "C" fn() -> i32;
type DescribeFn = unsafe extern "C" fn(*const u8, usize) -> NativeBytes;
type CreateFn = unsafe extern "C" fn(*const u8, usize) -> i32;
type DestroyFn = unsafe extern "C" fn(i32);
type PrepareFn = unsafe extern "C" fn(i32, i32, i32, i32) -> i32;
type SetParamFn = unsafe extern "C" fn(i32, *const u8, usize, i32, f64) -> i32;
type ProcessFn = unsafe extern "C" fn(i32, *mut f32, i32, *const u8, i32, i64, i32) -> i32;
type LatencyFn = unsafe extern "C" fn(i32) -> i32;
type DrawFn = unsafe extern "C" fn(i32, *const u8, usize) -> NativeBytes;

/// A loaded native extension shared library.
pub(crate) struct NativeLibrary {
    _library: Library,
    describe: DescribeFn,
    create: CreateFn,
    destroy: DestroyFn,
    prepare: PrepareFn,
    set_param: SetParamFn,
    process: ProcessFn,
    latency: LatencyFn,
    draw: DrawFn,
}

impl NativeLibrary {
    pub(crate) fn load(path: &Path) -> Result<Arc<Self>, ExtensionError> {
        let library = unsafe { Library::new(path) }
            .map_err(|error| ExtensionError::Native(format!("load {}: {error}", path.display())))?;
        unsafe {
            let version: AbiVersionFn = *library
                .get(b"notwig_abi_version\0")
                .map_err(|error| missing_export("notwig_abi_version", error))?;
            let expected = i32::try_from(ABI_VERSION)
                .map_err(|_| ExtensionError::Abi("ABI version does not fit in i32".into()))?;
            let found = version();
            if found != expected {
                return Err(ExtensionError::Abi(format!(
                    "extension ABI {found} does not match host {ABI_VERSION}"
                )));
            }
            Ok(Arc::new(Self {
                describe: *library
                    .get(b"notwig_device_describe\0")
                    .map_err(|error| missing_export("notwig_device_describe", error))?,
                create: *library
                    .get(b"notwig_device_create\0")
                    .map_err(|error| missing_export("notwig_device_create", error))?,
                destroy: *library
                    .get(b"notwig_device_destroy\0")
                    .map_err(|error| missing_export("notwig_device_destroy", error))?,
                prepare: *library
                    .get(b"notwig_device_prepare\0")
                    .map_err(|error| missing_export("notwig_device_prepare", error))?,
                set_param: *library
                    .get(b"notwig_device_set_param\0")
                    .map_err(|error| missing_export("notwig_device_set_param", error))?,
                process: *library
                    .get(b"notwig_device_process\0")
                    .map_err(|error| missing_export("notwig_device_process", error))?,
                latency: *library
                    .get(b"notwig_device_latency\0")
                    .map_err(|error| missing_export("notwig_device_latency", error))?,
                draw: *library
                    .get(b"notwig_device_draw\0")
                    .map_err(|error| missing_export("notwig_device_draw", error))?,
                _library: library,
            }))
        }
    }
}

fn missing_export(name: &str, error: impl std::fmt::Display) -> ExtensionError {
    ExtensionError::MissingExport(format!("{name}: {error}"))
}

/// A native device instance created from a loaded library.
pub(crate) struct NativeInstance {
    library: Arc<NativeLibrary>,
    handle: i32,
}

impl Drop for NativeInstance {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                (self.library.destroy)(self.handle);
            }
        }
    }
}

impl NativeInstance {
    pub(crate) fn new(library: Arc<NativeLibrary>) -> Self {
        Self { library, handle: 0 }
    }

    pub(crate) fn describe(
        &mut self,
        type_id: &str,
    ) -> Result<notwig_dsp::DeviceSpec, ExtensionError> {
        let bytes = unsafe { (self.library.describe)(type_id.as_ptr(), type_id.len()) };
        let json = read_bytes(bytes)?;
        serde_json::from_slice(&json)
            .map_err(|error| ExtensionError::Abi(format!("parse device spec: {error}")))
    }

    pub(crate) fn create_device(&mut self, type_id: &str) -> Result<i32, ExtensionError> {
        let handle = unsafe { (self.library.create)(type_id.as_ptr(), type_id.len()) };
        if handle < 0 {
            return Err(ExtensionError::InvalidHandle);
        }
        self.handle = handle;
        Ok(handle)
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
        let status =
            unsafe { (self.library.prepare)(handle, sample_rate, max_block, i32::from(channels)) };
        check_status(status)
    }

    pub(crate) fn set_param(
        &mut self,
        handle: i32,
        param_id: notwig_core::ParamId,
        value: &ParamValue,
    ) -> Result<(), ExtensionError> {
        let id = param_id.to_string();
        let (tag, raw) = param_wire(value);
        let status = unsafe { (self.library.set_param)(handle, id.as_ptr(), id.len(), tag, raw) };
        check_status(status)
    }

    pub(crate) fn process(
        &mut self,
        handle: i32,
        audio_ptr: *mut f32,
        frames: usize,
        events_ptr: *const u8,
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
        Ok(unsafe {
            (self.library.process)(
                handle,
                audio_ptr,
                frames,
                events_ptr,
                event_count,
                sample_position,
                i32::from(playing),
            )
        })
    }

    pub(crate) fn latency(&mut self, handle: i32) -> u32 {
        let samples = unsafe { (self.library.latency)(handle) };
        u32::try_from(samples).unwrap_or(0)
    }

    pub(crate) fn draw_panel(
        &mut self,
        handle: i32,
        context: &DeviceDrawContext,
    ) -> Result<PanelDraw, ExtensionError> {
        let json = serde_json::to_vec(context)
            .map_err(|error| ExtensionError::Abi(format!("encode draw context: {error}")))?;
        let bytes = unsafe { (self.library.draw)(handle, json.as_ptr(), json.len()) };
        let json = read_bytes(bytes)?;
        let panel: PanelDraw = serde_json::from_slice(&json)
            .map_err(|error| ExtensionError::Abi(format!("parse panel draw: {error}")))?;
        panel
            .validate()
            .map_err(|error| ExtensionError::Abi(error.to_string()))?;
        Ok(panel)
    }
}

fn read_bytes(bytes: NativeBytes) -> Result<Vec<u8>, ExtensionError> {
    if bytes.ptr.is_null() || bytes.len == 0 {
        return Err(ExtensionError::Abi("empty result".into()));
    }
    if bytes.len > MAX_OUTPUT_BYTES {
        return Err(ExtensionError::Abi("result is too large".into()));
    }
    let data = unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) };
    Ok(data.to_vec())
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
        Some(other) => Err(ExtensionError::Native(format!(
            "extension returned {other:?}"
        ))),
        None => Err(ExtensionError::Abi(format!(
            "extension returned unknown status {status}"
        ))),
    }
}
