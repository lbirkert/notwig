use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::abi::AbiStatus;

struct Inner<T> {
    next_handle: i32,
    devices: BTreeMap<i32, Arc<Mutex<T>>>,
}

/// Handle-keyed storage for extension device instances.
///
/// `new` is const so extensions can declare a single static store without a
/// `OnceLock`. Handles start at 1 and are never reused.
///
/// Each instance has its own lock, so audio processing for one device never
/// waits on panel drawing for another.
pub struct DeviceStore<T> {
    inner: Mutex<Inner<T>>,
}

impl<T> DeviceStore<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_handle: 0,
                devices: BTreeMap::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn create(&self, device: T) -> i32 {
        let mut inner = self.lock();
        inner.next_handle = inner.next_handle.wrapping_add(1).max(1);
        let handle = inner.next_handle;
        inner.devices.insert(handle, Arc::new(Mutex::new(device)));
        handle
    }

    pub fn remove(&self, handle: i32) -> Option<T> {
        let slot = self.lock().devices.remove(&handle)?;
        let mutex = Arc::try_unwrap(slot).ok()?;
        Some(
            mutex
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Runs `f` against the device for `handle`.
    ///
    /// Returns `AbiStatus::UnknownDevice` when the handle is not present.
    pub fn with<R>(&self, handle: i32, f: impl FnOnce(&mut T) -> R) -> Result<R, AbiStatus> {
        let slot = {
            let inner = self.lock();
            inner.devices.get(&handle).cloned()
        };
        let Some(slot) = slot else {
            return Err(AbiStatus::UnknownDevice);
        };
        let mut device = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(f(&mut device))
    }
}

impl<T> Default for DeviceStore<T> {
    fn default() -> Self {
        Self::new()
    }
}
