use lv2_raw::core::LV2Feature;
use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle};

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;

pub(crate) const LV2_URID_MAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";

/// Bidirectional URI ↔ u32 map handed to LV2 plugins as the
/// standard `urid#map` feature.
pub(crate) struct UridMap {
    entries: Mutex<HashMap<CString, LV2Urid>>,
    next_id: Mutex<LV2Urid>,
}

impl UridMap {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            // Reserve 0 as the LV2-spec "couldn't map" sentinel.
            next_id: Mutex::new(1),
        }
    }

    pub(crate) fn map_uri(&self, uri: &CStr) -> LV2Urid {
        let mut entries = self.entries.lock().expect("urid map mutex");
        if let Some(&id) = entries.get(uri) {
            return id;
        }
        let mut next = self.next_id.lock().expect("urid next mutex");
        let id = *next;
        *next += 1;
        entries.insert(uri.to_owned(), id);
        id
    }
}

pub(crate) extern "C" fn map_callback(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    // SAFETY: `handle` is the `&UridMap` we set into the feature
    // struct at instantiate time; lilv passes it back unchanged.
    let map = unsafe { &*(handle as *const UridMap) };
    let cstr = unsafe { CStr::from_ptr(uri) };
    map.map_uri(cstr)
}

/// Self-contained LV2 feature list. Heap-allocated so the pointers
/// fed to `lilv_plugin_instantiate` stay valid for the instance's
/// lifetime even if `Lv2Plugin` is moved.
pub(crate) struct Features {
    pub(crate) urid_map: UridMap,
    // `LV2UridMap.handle` points at `urid_map` — the box's pinned
    // address. `LV2UridMap.map` is the static callback. Held inline
    // so its address can be taken for `feature.data`.
    map_struct: LV2UridMap,
    // The features array proper. Each `data` field points at a
    // sibling field of the same `Features` allocation.
    feature_storage: Vec<LV2Feature>,
    // Null-terminated pointer array — what `lilv_plugin_instantiate`
    // actually consumes.
    pub(crate) feature_ptrs: Vec<*const LV2Feature>,
}

impl Features {
    pub(crate) fn new() -> Box<Self> {
        // Build in two passes so the `Box`'s heap address is stable
        // before we record interior pointers.
        let mut boxed = Box::new(Features {
            urid_map: UridMap::new(),
            map_struct: LV2UridMap {
                handle: std::ptr::null_mut(),
                map: map_callback,
            },
            feature_storage: Vec::with_capacity(1),
            feature_ptrs: Vec::with_capacity(2),
        });
        let urid_map_handle: *mut UridMap = &mut boxed.urid_map as *mut UridMap;
        boxed.map_struct.handle = urid_map_handle.cast::<c_void>();
        boxed.feature_storage.push(LV2Feature {
            uri: LV2_URID_MAP_URI.as_ptr().cast::<c_char>(),
            data: (&boxed.map_struct) as *const LV2UridMap as *mut c_void,
        });
        boxed.feature_ptrs.push(&boxed.feature_storage[0]);
        boxed.feature_ptrs.push(std::ptr::null());
        boxed
    }
}
