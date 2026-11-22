#![allow(unsafe_code)]

use notwig_core::ParamId;

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_AUDIO_SAMPLES: usize = 1 << 20;

/// Reads a UTF-8 string from extension linear memory and passes it to `f`.
///
/// The host is responsible for passing a valid pointer/length pair into the
/// module's memory. Invalid or oversized inputs return `None`.
pub fn with_str<R>(ptr: i32, len: i32, f: impl FnOnce(&str) -> R) -> Option<R> {
    if ptr <= 0 || len < 0 {
        return None;
    }
    let len = usize::try_from(len).ok()?;
    if len > MAX_INPUT_BYTES {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    let text = std::str::from_utf8(bytes).ok()?;
    Some(f(text))
}

/// Calls `f` with a byte slice from extension linear memory.
///
/// The host is responsible for passing a valid pointer/length pair into the
/// module's memory. Invalid or oversized inputs return `None`.
pub fn with_bytes<R>(ptr: i32, len: i32, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if ptr <= 0 || len < 0 {
        return None;
    }
    let len = usize::try_from(len).ok()?;
    if len > MAX_INPUT_BYTES {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    Some(f(bytes))
}

/// Parses a parameter UUID from extension linear memory without allocating.
pub fn read_param_id(ptr: i32, len: i32) -> Option<ParamId> {
    with_str(ptr, len, |text| text.parse().ok()).flatten()
}

/// Reads a UTF-8 string from a host-provided native pointer.
///
/// Used by the native shared-library ABI, where the host passes ordinary
/// process pointers instead of offsets into WASM linear memory.
pub fn with_str_ptr<R>(ptr: *const u8, len: usize, f: impl FnOnce(&str) -> R) -> Option<R> {
    let text = with_bytes_ptr(ptr, len, |bytes| std::str::from_utf8(bytes).ok().map(f));
    text.flatten()
}

/// Calls `f` with a byte slice from a host-provided native pointer.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn with_bytes_ptr<R>(ptr: *const u8, len: usize, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if ptr.is_null() || len > MAX_INPUT_BYTES {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(f(bytes))
}

/// Parses a parameter UUID from a host-provided native pointer.
pub fn read_param_id_ptr(ptr: *const u8, len: usize) -> Option<ParamId> {
    with_str_ptr(ptr, len, |text| text.parse().ok()).flatten()
}

/// Calls `f` with a mutable interleaved `f32` audio buffer from a host
/// pointer.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn with_audio_frames_ptr<R>(
    ptr: *mut f32,
    frames: usize,
    channels: usize,
    f: impl FnOnce(&mut [f32]) -> R,
) -> Option<R> {
    let samples = frames.checked_mul(channels)?;
    if ptr.is_null()
        || samples == 0
        || samples > MAX_AUDIO_SAMPLES
        || (ptr as usize) % std::mem::align_of::<f32>() != 0
    {
        return None;
    }
    let audio = unsafe { std::slice::from_raw_parts_mut(ptr, samples) };
    Some(f(audio))
}

/// Calls `f` with a mutable interleaved `f32` audio buffer.
///
/// `ptr` must point to `samples` f32 values in extension memory and be
/// 4-byte aligned. Hosts allocate audio through [`crate::abi::heap::alloc`],
/// which guarantees 8-byte alignment.
pub fn with_audio_buffer<R>(
    ptr: i32,
    samples: usize,
    f: impl FnOnce(&mut [f32]) -> R,
) -> Option<R> {
    if ptr <= 0
        || samples == 0
        || samples > MAX_AUDIO_SAMPLES
        || ptr as usize % std::mem::align_of::<f32>() != 0
    {
        return None;
    }
    let audio = unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, samples) };
    Some(f(audio))
}

/// Calls `f` with `frames * channels` interleaved samples.
pub fn with_audio_frames<R>(
    ptr: i32,
    frames: usize,
    channels: usize,
    f: impl FnOnce(&mut [f32]) -> R,
) -> Option<R> {
    let samples = frames.checked_mul(channels)?;
    with_audio_buffer(ptr, samples, f)
}
