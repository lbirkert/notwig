#![allow(unsafe_code)]

use notwig_core::ParamId;

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_AUDIO_SAMPLES: usize = 1 << 20;

/// Reads a UTF-8 string from extension linear memory and passes it to `f`.
///
/// This is a safe wrapper, but the caller must uphold the raw-pointer
/// contract below; the function cannot verify provenance.
///
/// # ABI
///
/// `ptr` is a nonzero byte offset into the guest module's linear memory and
/// `len` is the byte length. The region must be readable for `len` bytes and
/// contain valid UTF-8. Negative pointers, zero-length-adjacent invalid values,
/// inputs larger than 1 MiB, or invalid UTF-8 return `None`.
///
/// The safe signature is sound only for guest-managed offsets produced by the
/// generated ABI. Host code must validate and copy through the runtime's memory
/// API instead of calling this with an untrusted or host pointer.
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
/// # ABI
///
/// `ptr` is a nonzero byte offset into the guest module's linear memory and
/// `len` is the byte length. The region must be readable for `len` bytes.
/// Negative pointers and inputs larger than 1 MiB return `None`. The slice is
/// only valid for the duration of the call; `f` must not retain it.
///
/// The safe signature is sound only for guest-managed offsets produced by the
/// generated ABI. Host code must validate and copy through the runtime's memory
/// API instead of calling this with an untrusted or host pointer.
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
///
/// # ABI
///
/// Follows the same guest-memory contract as [`with_str`]. Returns `None` when
/// the region is invalid or its text is not a valid `ParamId`.
pub fn read_param_id(ptr: i32, len: i32) -> Option<ParamId> {
    with_str(ptr, len, |text| text.parse().ok()).flatten()
}

/// Reads a UTF-8 string from a host-provided native pointer.
///
/// Used by the native shared-library ABI, where the host passes ordinary
/// process pointers instead of offsets into WASM linear memory.
///
/// # Safety
///
/// The caller must guarantee that `ptr` points to `len` initialized bytes that
/// stay valid for the duration of the call. The function checks for null and
/// the 1 MiB input limit, but cannot verify provenance or alignment. Invalid
/// UTF-8 returns `None`.
pub unsafe fn with_str_ptr<R>(ptr: *const u8, len: usize, f: impl FnOnce(&str) -> R) -> Option<R> {
    let text = unsafe { with_bytes_ptr(ptr, len, |bytes| std::str::from_utf8(bytes).ok().map(f)) };
    text.flatten()
}

/// Calls `f` with a byte slice from a host-provided native pointer.
///
/// # Safety
///
/// The caller must guarantee that `ptr` points to `len` initialized bytes that
/// stay valid for the duration of the call. Null pointers and inputs larger than
/// 1 MiB return `None`; provenance and alignment are not checked because this
/// path is used for arbitrary native command buffers.
pub unsafe fn with_bytes_ptr<R>(
    ptr: *const u8,
    len: usize,
    f: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    if ptr.is_null() || len > MAX_INPUT_BYTES {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(f(bytes))
}

/// Parses a parameter UUID from a host-provided native pointer.
///
/// # Safety
///
/// Follows the same raw-pointer contract as [`with_str_ptr`]. Returns `None`
/// when the pointer is null, the input is too large, or the text is not a valid
/// `ParamId`.
pub unsafe fn read_param_id_ptr(ptr: *const u8, len: usize) -> Option<ParamId> {
    unsafe { with_str_ptr(ptr, len, |text| text.parse().ok()) }.flatten()
}

/// Calls `f` with a mutable interleaved `f32` audio buffer from a host
/// pointer.
///
/// # Safety
///
/// `ptr` must be valid for exclusive access to `frames * channels` `f32`
/// samples for the duration of the call, must not alias other live references,
/// and must be 4-byte aligned. The function validates non-nullness, the
/// multiplication overflow, the 1 MiB sample limit, and alignment, but cannot
/// verify provenance. Returns `None` when those checks fail.
pub unsafe fn with_audio_frames_ptr<R>(
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
/// which guarantees 8-byte alignment. The slice is only valid for the duration
/// of the call; `f` must not retain it.
///
/// # ABI
///
/// `ptr` is a nonzero byte offset into the guest module's linear memory. This
/// wrapper checks alignment, non-empty input, multiplication results, and the
/// 1 MiB sample limit, but cannot verify that the guest region is mapped. Its
/// safe signature is sound only for guest-managed offsets; host code must pass a
/// validated buffer through the runtime instead.
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
///
/// # ABI
///
/// Follows [`with_audio_buffer`] after computing `frames * channels`. Returns
/// `None` on multiplication overflow or when the buffer checks fail.
pub fn with_audio_frames<R>(
    ptr: i32,
    frames: usize,
    channels: usize,
    f: impl FnOnce(&mut [f32]) -> R,
) -> Option<R> {
    let samples = frames.checked_mul(channels)?;
    with_audio_buffer(ptr, samples, f)
}
