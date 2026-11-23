//! In-memory `IBStream` implementation for state save/load.

use vst3::Class;
use vst3::Steinberg::{IBStream, IBStreamTrait, kResultOk};

/// Backing storage for the `IBStream` we hand to
/// `IComponent::setState` / `getState`. The plugin reads and
/// writes through `read`/`write`; the host inspects
/// `data` / `position` after the call.
#[derive(Default)]
pub(crate) struct MemoryStream {
    pub(crate) data: std::cell::RefCell<Vec<u8>>,
    pub(crate) position: std::cell::Cell<usize>,
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: i32,
        num_bytes_read: *mut i32,
    ) -> i32 {
        if buffer.is_null() || num_bytes < 0 {
            return -1;
        }
        let pos = self.position.get();
        #[allow(clippy::cast_sign_loss)]
        let want = num_bytes as usize;
        let data = self.data.borrow();
        let available = data.len().saturating_sub(pos);
        let take = want.min(available);
        if take > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(pos), buffer.cast::<u8>(), take);
            }
        }
        self.position.set(pos + take);
        if !num_bytes_read.is_null() {
            unsafe {
                *num_bytes_read = i32::try_from(take).unwrap_or(i32::MAX);
            }
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: i32,
        num_bytes_written: *mut i32,
    ) -> i32 {
        if buffer.is_null() || num_bytes < 0 {
            return -1;
        }
        let pos = self.position.get();
        #[allow(clippy::cast_sign_loss)]
        let want = num_bytes as usize;
        let mut data = self.data.borrow_mut();
        if data.len() < pos + want {
            data.resize(pos + want, 0);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buffer.cast::<u8>(), data.as_mut_ptr().add(pos), want);
        }
        self.position.set(pos + want);
        if !num_bytes_written.is_null() {
            unsafe {
                *num_bytes_written = i32::try_from(want).unwrap_or(i32::MAX);
            }
        }
        kResultOk
    }

    #[allow(clippy::cast_possible_wrap)]
    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> i32 {
        // VST3 SDK SeekMode: 0 = `SeekSet`, 1 = `SeekCur`, 2 = `SeekEnd`.
        let data_len = self.data.borrow().len() as i64;
        let current = self.position.get() as i64;
        let new_pos = match mode {
            0 => pos,
            1 => current + pos,
            2 => data_len + pos,
            _ => return -1,
        };
        if new_pos < 0 {
            return -1;
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        self.position.set(new_pos as usize);
        if !result.is_null() {
            unsafe {
                *result = new_pos;
            }
        }
        kResultOk
    }

    #[allow(clippy::cast_possible_wrap)]
    unsafe fn tell(&self, pos: *mut i64) -> i32 {
        if pos.is_null() {
            return -1;
        }
        unsafe {
            *pos = self.position.get() as i64;
        }
        kResultOk
    }
}
