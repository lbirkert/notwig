pub mod heap;
pub mod pointers;
pub mod store;

pub use heap::{alloc, free, packed_pointer};
pub use pointers::{
    read_param_id, read_param_id_ptr, with_audio_buffer, with_audio_frames, with_audio_frames_ptr,
    with_bytes, with_bytes_ptr, with_str, with_str_ptr,
};
pub use store::DeviceStore;

/// Current extension ABI version.
///
/// Hosts require an exact match with the version reported by the extension. Any
/// change to an export signature, wire layout, or documented contract must bump
/// this value; otherwise mismatched extensions are rejected at load time.
pub const ABI_VERSION: u32 = 9;

/// Maximum serialized metadata response from an extension.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Status code returned by generated ABI exports.
///
/// The numeric values are part of the ABI and are returned as `i32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum AbiStatus {
    /// The operation succeeded.
    Ok = 0,
    /// A generic extension-side failure.
    Error = -1,
    /// The caller-provided output buffer was too small.
    BufferTooSmall = -2,
    /// No device instance exists for the provided handle.
    UnknownDevice = -3,
    /// An argument was malformed, such as invalid UTF-8 or a bad pointer.
    InvalidArgument = -4,
    /// The WASM instance exhausted its execution fuel.
    FuelExhausted = -5,
}

impl AbiStatus {
    /// Returns the numeric ABI status code.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    /// Decodes a numeric ABI status code, returning `None` when unknown.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            -1 => Some(Self::Error),
            -2 => Some(Self::BufferTooSmall),
            -3 => Some(Self::UnknownDevice),
            -4 => Some(Self::InvalidArgument),
            -5 => Some(Self::FuelExhausted),
            _ => None,
        }
    }
}

/// Packs a 32-bit guest pointer and length into one 64-bit return value.
///
/// The pointer occupies the high 32 bits and the length the low 32 bits. This
/// is the return layout used by the WASM `notwig_device_describe` and
/// `notwig_device_draw` exports.
#[must_use]
pub const fn pack_ptr_len(ptr: u32, len: u32) -> u64 {
    (ptr as u64) << 32 | len as u64
}

/// Splits a value produced by [`pack_ptr_len`] into `(ptr, len)`.
#[must_use]
pub const fn unpack_ptr_len(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}
