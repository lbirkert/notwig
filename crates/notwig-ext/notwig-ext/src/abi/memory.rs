//! Guest linear-memory access and packed result handling.

use notwig_ext_api::{MAX_OUTPUT_BYTES, unpack_ptr_len};

use super::instance::ExtensionInstance;
use crate::error::{ExtensionError, wasm_err};

const MAX_INPUT_BYTES: usize = 1024 * 1024;

impl ExtensionInstance {
    pub(crate) fn alloc_audio(&mut self, bytes: usize) -> Result<i32, ExtensionError> {
        let len = i32::try_from(bytes)
            .map_err(|_| ExtensionError::Abi("audio buffer is too large".into()))?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(wasm_err)?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        Ok(ptr)
    }

    pub(crate) fn write_bytes(&mut self, ptr: i32, bytes: &[u8]) -> Result<(), ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        self.memory
            .write(&mut self.store, start, bytes)
            .map_err(wasm_err)
    }

    pub(crate) fn read_into(&self, ptr: i32, out: &mut [u8]) -> Result<(), ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let end = start
            .checked_add(out.len())
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        if end > self.memory.data_size(&self.store) {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        self.memory.read(&self.store, start, out).map_err(wasm_err)
    }

    pub(super) fn read_bytes(&self, ptr: i32, len: i32) -> Result<Vec<u8>, ExtensionError> {
        let start =
            usize::try_from(ptr).map_err(|_| ExtensionError::Abi("negative pointer".into()))?;
        let len =
            usize::try_from(len).map_err(|_| ExtensionError::Abi("negative length".into()))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| ExtensionError::Abi("pointer overflow".into()))?;
        let data = self.memory.data(&self.store);
        if end > data.len() {
            return Err(ExtensionError::Abi("out-of-bounds read".into()));
        }
        Ok(data[start..end].to_vec())
    }

    pub(super) fn write_input(&mut self, bytes: &[u8]) -> Result<(i32, i32), ExtensionError> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(ExtensionError::Abi("input is too large".into()));
        }
        let len = i32::try_from(bytes.len())
            .map_err(|_| ExtensionError::Abi("input is too large".into()))?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(wasm_err)?;
        if ptr < 0 {
            return Err(ExtensionError::BudgetExceeded);
        }
        self.write_bytes(ptr, bytes)?;
        Ok((ptr, len))
    }

    pub(super) fn free_input(&mut self, ptr: i32, len: i32) {
        let _ = self.free.call(&mut self.store, (ptr, len));
    }

    pub(super) fn read_packed_json(&mut self, packed: i64) -> Result<Vec<u8>, ExtensionError> {
        self.read_packed_bytes(packed, MAX_OUTPUT_BYTES)
    }

    pub(super) fn read_packed_bytes(
        &mut self,
        packed: i64,
        max: usize,
    ) -> Result<Vec<u8>, ExtensionError> {
        let (ptr, len) = unpack_ptr_len(packed as u64);
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
