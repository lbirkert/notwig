use lv2_raw::core::LV2Feature;
use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle};

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;

pub(crate) const LV2_URID_MAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";
pub(crate) const LV2_URID_UNMAP_URI: &[u8] = b"http://lv2plug.in/ns/ext/urid#unmap\0";

/// LV2 `urid:unmap` feature struct. `lv2_raw` 0.2 only declares
/// `LV2UridMap`, so mirror the tiny C layout locally.
#[repr(C)]
pub(crate) struct Lv2UridUnmap {
    pub(crate) handle: LV2UridMapHandle,
    pub(crate) unmap: extern "C" fn(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char,
}

/// Bidirectional URI ↔ u32 map handed to LV2 plugins as the
/// standard `urid#map` feature.
pub(crate) struct UridMap {
    entries: Mutex<HashMap<CString, LV2Urid>>,
    reverse: Mutex<HashMap<LV2Urid, Box<CStr>>>,
    next_id: Mutex<LV2Urid>,
}

impl UridMap {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            reverse: Mutex::new(HashMap::new()),
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
        drop(entries);
        self.reverse
            .lock()
            .expect("urid reverse mutex")
            .insert(id, uri.to_owned().into_boxed_c_str());
        id
    }

    fn uri_ptr(&self, urid: LV2Urid) -> *const c_char {
        self.reverse
            .lock()
            .expect("urid reverse mutex")
            .get(&urid)
            .map_or(std::ptr::null(), |uri| uri.as_ptr())
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

pub(crate) extern "C" fn unmap_callback(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char {
    if handle.is_null() {
        return std::ptr::null();
    }
    // SAFETY: `handle` is the `&UridMap` set in the feature struct.
    let map = unsafe { &*(handle as *const UridMap) };
    map.uri_ptr(urid)
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
    unmap_struct: Lv2UridUnmap,
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
            unmap_struct: Lv2UridUnmap {
                handle: std::ptr::null_mut(),
                unmap: unmap_callback,
            },
            feature_storage: Vec::with_capacity(2),
            feature_ptrs: Vec::with_capacity(3),
        });
        let urid_map_handle: *mut UridMap = &mut boxed.urid_map as *mut UridMap;
        boxed.map_struct.handle = urid_map_handle.cast::<c_void>();
        boxed.unmap_struct.handle = urid_map_handle.cast::<c_void>();
        boxed.feature_storage.push(LV2Feature {
            uri: LV2_URID_MAP_URI.as_ptr().cast::<c_char>(),
            data: (&boxed.map_struct) as *const LV2UridMap as *mut c_void,
        });
        boxed.feature_storage.push(LV2Feature {
            uri: LV2_URID_UNMAP_URI.as_ptr().cast::<c_char>(),
            data: (&boxed.unmap_struct) as *const Lv2UridUnmap as *mut c_void,
        });
        boxed.feature_ptrs.push(&boxed.feature_storage[0]);
        boxed.feature_ptrs.push(&boxed.feature_storage[1]);
        boxed.feature_ptrs.push(std::ptr::null());
        boxed
    }

    pub(crate) fn map_ptr(&self) -> *mut LV2UridMap {
        (&self.map_struct as *const LV2UridMap).cast_mut()
    }

    pub(crate) fn unmap_ptr(&self) -> *mut Lv2UridUnmap {
        (&self.unmap_struct as *const Lv2UridUnmap).cast_mut()
    }
}
