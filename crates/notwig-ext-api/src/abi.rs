pub mod heap;
pub mod pointers;
pub mod store;

pub use heap::{alloc, free, packed_pointer};
pub use pointers::{read_param_id, with_audio_buffer, with_audio_frames, with_str};
pub use store::DeviceStore;

pub const ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum AbiStatus {
    Ok = 0,
    Error = -1,
    BufferTooSmall = -2,
    UnknownDevice = -3,
    InvalidArgument = -4,
    FuelExhausted = -5,
}

impl AbiStatus {
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

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

#[must_use]
pub const fn pack_ptr_len(ptr: u32, len: u32) -> u64 {
    (ptr as u64) << 32 | len as u64
}

#[must_use]
pub const fn unpack_ptr_len(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}
