//! Typed-array access to the browser WASM linear memory.

use js_sys::Uint8Array;
use notwig_ext_api::{MAX_OUTPUT_BYTES, unpack_ptr_len};
use wasm_bindgen::JsValue;

use super::WebExtensionInstance;
use crate::error::ExtensionError;

const MAX_INPUT_BYTES: usize = 1024 * 1024;

impl WebExtensionInstance {
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

    pub(super) fn read_bytes(&self, ptr: i32, len: i32) -> Result<Vec<u8>, ExtensionError> {
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

    pub(super) fn write_input(&mut self, bytes: &[u8]) -> Result<(i32, i32), ExtensionError> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(ExtensionError::Abi("input is too large".into()));
        }
        let len = i32::try_from(bytes.len())
            .map_err(|_| ExtensionError::Abi("input is too large".into()))?;
        let ptr = super::abi::call_i32(&self.alloc, &[JsValue::from(len)])?;
        self.write_bytes(ptr, bytes)?;
        Ok((ptr, len))
    }

    pub(super) fn free_input(&mut self, ptr: i32, len: i32) {
        let _ = self.free.call2(
            &JsValue::UNDEFINED,
            &JsValue::from(ptr),
            &JsValue::from(len),
        );
    }

    pub(super) fn read_packed_json(&mut self, packed: u64) -> Result<Vec<u8>, ExtensionError> {
        self.read_packed_bytes(packed, MAX_OUTPUT_BYTES)
    }

    pub(super) fn read_packed_bytes(
        &mut self,
        packed: u64,
        max: usize,
    ) -> Result<Vec<u8>, ExtensionError> {
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
