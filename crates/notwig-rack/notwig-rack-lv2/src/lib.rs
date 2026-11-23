//! LV2 host implementation for the notwig-rack framework.
//!
//! Built on `lilv-sys` — the Rust FFI for **lilv**, the standard
//! LV2 host library. lilv handles URI resolution, TTL parsing, and
//! the Turtle world load that LV2 demands; notwig-rack-lv2 is the layer
//! that turns those C-API queries into
//! [`PluginInfo`](notwig_rack_core::info::PluginInfo) entries and
//! drives `lilv_plugin_instantiate` / `lilv_instance_run` from the
//! notwig-rack-core trait surface.
//!
//! # Status
//!
//! - **Scan** via `lilv_world_load_all`.
//! - **Load** instantiates the plugin into a `LilvInstance` with
//!   the LV2 `urid#map` feature. Each plugin owns its own world
//!   (instance pointers reference world-owned data).
//! - **Process** connects audio / control / atom-sequence ports
//!   per block and calls `lilv_instance_run`. Audio ports point
//!   straight at the host's planes (zero-copy); control ports
//!   carry their declared default value; the MIDI input atom port
//!   is rebuilt each block from the notwig-rack
//!   [`EventList`](notwig_rack_core::events::EventList).
//! - **MIDI** in via atom sequence ports tagged `midi:MidiEvent`.
//!   MIDI out is not yet drained back to the notwig-rack host.
//!
//! # Build dependency
//!
//! `lilv-sys` links against the system `lilv-0` library. On macOS,
//! `brew install lilv`. On Debian/Ubuntu, `apt install liblilv-dev`.
//! Without those, `cargo build -p notwig-rack-lv2` will fail at link
//! time.

#![allow(
    // Atom struct sizes are well under u32::MAX; the casts are for
    // FFI fields that are themselves u32.
    clippy::cast_possible_truncation,
    // Vec<u8> allocations come from the global allocator with at
    // least 8-byte alignment; safe to cast to atom-header pointers.
    clippy::cast_ptr_alignment,
    // `&x as *const _` reads cleaner here than `&raw const x`.
    clippy::borrow_as_ptr,
    clippy::ref_as_ptr,
    clippy::ptr_as_ptr
)]

mod features;
mod midi;
mod plugin;
mod ports;
mod scanner;
mod time_atom;
mod ui;

pub use plugin::Lv2Plugin;
pub use scanner::Lv2Scanner;

/// Format identifier used on returned
/// [`PluginInfo`](notwig_rack_core::info::PluginInfo).
pub const FORMAT: &str = "lv2";

#[cfg(target_os = "macos")]
pub(crate) const NATIVE_UI_CLASS_URI: &[u8] = b"http://lv2plug.in/ns/extensions/ui#CocoaUI\0";
#[cfg(target_os = "windows")]
pub(crate) const NATIVE_UI_CLASS_URI: &[u8] = b"http://lv2plug.in/ns/extensions/ui#WindowsUI\0";
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const NATIVE_UI_CLASS_URI: &[u8] = b"http://lv2plug.in/ns/extensions/ui#X11UI\0";
