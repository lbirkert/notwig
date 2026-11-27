use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::info::{PluginCategory, PluginInfo};
use notwig_rack_core::scanner::PluginScanner;

use std::ffi::{CStr, c_char};
use std::path::Path;

use crate::plugin::Lv2Plugin;
use crate::{FORMAT, NATIVE_UI_CLASS_URI};

/// LV2 scanner.
#[derive(Debug, Default)]
pub struct Lv2Scanner;

impl Lv2Scanner {
    /// Construct a default scanner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PluginScanner for Lv2Scanner {
    type Plugin = Lv2Plugin;

    fn scan(&self) -> Result<Vec<PluginInfo>> {
        let world = unsafe { World::new() }
            .ok_or_else(|| Error::Other("lilv_world_new returned NULL".into()))?;
        unsafe { lilv_sys::lilv_world_load_all(world.ptr) };
        Ok(unsafe { world.collect_plugin_infos() })
    }

    fn scan_path(&self, _path: &Path) -> Result<Vec<PluginInfo>> {
        // LV2's discovery model is URI-based, not path-based — the
        // host calls `lilv_world_load_all` and lilv consults
        // LV2_PATH / standard locations. A path-bounded scan would
        // need `lilv_world_load_bundle` against each subdirectory;
        // tracked as a follow-on.
        Err(Error::Other(
            "notwig-rack-lv2 path-bounded scan not yet implemented".into(),
        ))
    }

    fn load(&self, info: &PluginInfo) -> Result<Self::Plugin> {
        Lv2Plugin::load_from(info)
    }
}

// ---------------------------------------------------------------------------
// World wrapper
// ---------------------------------------------------------------------------

/// RAII wrapper around `*mut LilvWorld`. Holds the world for the
/// lifetime of either a scan or a loaded plugin (instance pointers
/// reference world-owned data, so the world must outlive them).
pub(crate) struct World {
    pub(crate) ptr: *mut lilv_sys::LilvWorld,
}

impl World {
    pub(crate) unsafe fn new() -> Option<Self> {
        let ptr = unsafe { lilv_sys::lilv_world_new() };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    unsafe fn collect_plugin_infos(&self) -> Vec<PluginInfo> {
        // Stash this world for the per-plugin classifiers to reach
        // (lilv exposes no `plugin → world` accessor). Cleared
        // before we return so the TLS pointer doesn't outlive
        // the borrow.
        SCAN_WORLD.with(|w| w.set(self.ptr));
        let mut out = Vec::new();
        let plugins = unsafe { lilv_sys::lilv_world_get_all_plugins(self.ptr) };
        if plugins.is_null() {
            SCAN_WORLD.with(|w| w.set(std::ptr::null_mut()));
            return out;
        }
        let audio_uri = unsafe { self.new_uri(lilv_sys::LILV_URI_AUDIO_PORT.as_ptr().cast()) };
        let input_uri = unsafe { self.new_uri(lilv_sys::LILV_URI_INPUT_PORT.as_ptr().cast()) };
        let output_uri = unsafe { self.new_uri(lilv_sys::LILV_URI_OUTPUT_PORT.as_ptr().cast()) };
        let atom_uri = unsafe { self.new_uri(lilv_sys::LILV_URI_ATOM_PORT.as_ptr().cast()) };
        let mut it = unsafe { lilv_sys::lilv_plugins_begin(plugins) };
        loop {
            if unsafe { lilv_sys::lilv_plugins_is_end(plugins, it) } {
                break;
            }
            let plugin = unsafe { lilv_sys::lilv_plugins_get(plugins, it) };
            if !plugin.is_null() {
                out.push(unsafe {
                    plugin_to_info(plugin, audio_uri, input_uri, output_uri, atom_uri)
                });
            }
            it = unsafe { lilv_sys::lilv_plugins_next(plugins, it) };
        }
        unsafe {
            lilv_sys::lilv_node_free(audio_uri);
            lilv_sys::lilv_node_free(input_uri);
            lilv_sys::lilv_node_free(output_uri);
            lilv_sys::lilv_node_free(atom_uri);
        }
        SCAN_WORLD.with(|w| w.set(std::ptr::null_mut()));
        out
    }

    unsafe fn new_uri(&self, uri: *const c_char) -> *mut lilv_sys::LilvNode {
        unsafe { lilv_sys::lilv_new_uri(self.ptr, uri.cast()) }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        unsafe { lilv_sys::lilv_world_free(self.ptr) };
    }
}

// SAFETY: lilv's world is meant to be single-threaded for mutation
// but we hand the wrapper between activate/process on the same
// thread the host owns. We never share it.
unsafe impl Send for World {}

unsafe fn plugin_to_info(
    plugin: *const lilv_sys::LilvPlugin,
    audio_uri: *mut lilv_sys::LilvNode,
    input_uri: *mut lilv_sys::LilvNode,
    output_uri: *mut lilv_sys::LilvNode,
    atom_uri: *mut lilv_sys::LilvNode,
) -> PluginInfo {
    let uri_node = unsafe { lilv_sys::lilv_plugin_get_uri(plugin) };
    let uri = unsafe { node_to_uri_string(uri_node) };

    let name_node = unsafe { lilv_sys::lilv_plugin_get_name(plugin) };
    let name = unsafe { node_to_string_owned(name_node) };
    unsafe { lilv_sys::lilv_node_free(name_node) };

    let author_node = unsafe { lilv_sys::lilv_plugin_get_author_name(plugin) };
    let vendor = if author_node.is_null() {
        String::new()
    } else {
        let v = unsafe { node_to_string_owned(author_node) };
        unsafe { lilv_sys::lilv_node_free(author_node) };
        v
    };

    let mut audio_in = 0u32;
    let mut audio_out = 0u32;
    let mut accepts_midi = false;
    let count = unsafe { lilv_sys::lilv_plugin_get_num_ports(plugin) };
    for idx in 0..count {
        let port = unsafe { lilv_sys::lilv_plugin_get_port_by_index(plugin, idx) };
        if port.is_null() {
            continue;
        }
        let is_input = unsafe { lilv_sys::lilv_port_is_a(plugin, port, input_uri) };
        if unsafe { lilv_sys::lilv_port_is_a(plugin, port, audio_uri) } {
            if is_input {
                audio_in += 1;
            } else if unsafe { lilv_sys::lilv_port_is_a(plugin, port, output_uri) } {
                audio_out += 1;
            }
        } else if is_input && unsafe { lilv_sys::lilv_port_is_a(plugin, port, atom_uri) } {
            // Best-effort: any atom-port input is treated as a MIDI
            // sink for the catalog. The actual MIDI-vs-not check
            // requires walking the port's `atom:supports` triples,
            // which we do at load time.
            accepts_midi = true;
        }
    }
    let category = if audio_in == 0 && audio_out > 0 {
        PluginCategory::Instrument
    } else {
        PluginCategory::Effect
    };

    let has_editor = unsafe { has_native_ui(plugin) };

    PluginInfo {
        name,
        vendor,
        version: 0,
        category,
        path: std::path::PathBuf::new(),
        unique_id: uri,
        format: FORMAT,
        has_editor,
        accepts_midi,
        supports_ara: false,
    }
}

/// True if the plugin advertises a UI of this platform's native class
/// (`CocoaUI` / `WindowsUI` / `X11UI`).
unsafe fn has_native_ui(plugin: *const lilv_sys::LilvPlugin) -> bool {
    let uis = unsafe { lilv_sys::lilv_plugin_get_uis(plugin) };
    if uis.is_null() {
        return false;
    }
    let world_ptr = unsafe { lilv_plugin_world(plugin) };
    if world_ptr.is_null() {
        unsafe { lilv_sys::lilv_uis_free(uis) };
        return false;
    }
    let class_node =
        unsafe { lilv_sys::lilv_new_uri(world_ptr, NATIVE_UI_CLASS_URI.as_ptr().cast()) };
    let mut found = false;
    let mut it = unsafe { lilv_sys::lilv_uis_begin(uis) };
    while !unsafe { lilv_sys::lilv_uis_is_end(uis, it) } {
        let ui = unsafe { lilv_sys::lilv_uis_get(uis, it) };
        if !ui.is_null() && unsafe { lilv_sys::lilv_ui_is_a(ui, class_node) } {
            found = true;
            break;
        }
        it = unsafe { lilv_sys::lilv_uis_next(uis, it) };
    }
    unsafe {
        lilv_sys::lilv_node_free(class_node);
        lilv_sys::lilv_uis_free(uis);
    }
    found
}

/// Recover the world pointer from a plugin pointer. lilv stores it
/// internally but doesn't expose it directly; we cheat by stashing
/// it into a thread-local during `collect_plugin_infos`.
unsafe fn lilv_plugin_world(_plugin: *const lilv_sys::LilvPlugin) -> *mut lilv_sys::LilvWorld {
    SCAN_WORLD.with(std::cell::Cell::get)
}

thread_local! {
    /// Set by `collect_plugin_infos` for the duration of one scan
    /// pass so the per-plugin classifiers can `lilv_new_uri`
    /// against the same world. Cleared on Drop of the World guard.
    static SCAN_WORLD: std::cell::Cell<*mut lilv_sys::LilvWorld> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
}

pub(crate) unsafe fn node_to_uri_string(node: *const lilv_sys::LilvNode) -> String {
    if node.is_null() {
        return String::new();
    }
    let p = unsafe { lilv_sys::lilv_node_as_uri(node) };
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

unsafe fn node_to_string_owned(node: *mut lilv_sys::LilvNode) -> String {
    if node.is_null() {
        return String::new();
    }
    let p = unsafe { lilv_sys::lilv_node_as_string(node) };
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}
