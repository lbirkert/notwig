//! Cross-format audio plugin host.
//!
//! `notwig-rack` is the umbrella crate. It re-exports the
//! per-format wrapper crates behind features so a downstream user
//! can opt into exactly the formats they need without juggling
//! version pins across multiple direct dependencies.
//!
//! # Default features
//!
//! `clap` + `vst3` + `vst2` — the cross-platform formats this fork
//! ships. CLAP is pure-Rust, VST3 uses the community `vst3` crate
//! (no Steinberg SDK submodule), and VST2 uses a small clean-room
//! ABI in [`notwig_rack_vst2`].
//!
//! # Opt-in features
//!
//! - `lv2` — LV2 host (requires the system `lilv-0` library)
//! - `au` — Audio Unit v2 host (Apple platforms only)
//! - `au3` — Audio Unit v3 host (Apple platforms only)
//!
//! # Layout
//!
//! Format wrappers are re-exported as nested modules so existing
//! `notwig_rack_clap::*` paths can also be reached as
//! `notwig_rack::clap::*`:
//!
//! ```ignore
//! use notwig_rack::core::scanner::PluginScanner;
//! use notwig_rack::clap::ClapScanner;
//!
//! let plugins = ClapScanner::new().scan()?;
//! ```
//!
//! For per-format granular dependency management, depend on the
//! individual `notwig-rack-*` crates directly instead.

#![doc(html_root_url = "https://docs.rs/notwig-rack/1.1.5")]
// Empty `cargo doc` build with no features enabled would emit
// dead-imports lints; suppress them at the crate level so the
// no-feature `cargo check` stays clean.
#![allow(unused_imports)]

/// Format-agnostic traits and types — `Plugin`, `PluginScanner`,
/// `PluginEditor`, the `EventList` / `MidiData` enums, and shared
/// error / info structs. Always available regardless of features.
pub use notwig_rack_core as core;

/// CLAP host. Enabled by default; disable with `default-features = false`.
#[cfg(feature = "clap")]
pub use notwig_rack_clap as clap;

/// VST3 host. Enabled by default; disable with `default-features = false`.
#[cfg(feature = "vst3")]
pub use notwig_rack_vst3 as vst3;

/// VST 2.4 host. Enabled by default; disable with `default-features = false`.
#[cfg(feature = "vst2")]
pub use notwig_rack_vst2 as vst2;

/// LV2 host. Enable with the `lv2` feature. Requires the system `lilv-0`
/// library at link time.
#[cfg(feature = "lv2")]
pub use notwig_rack_lv2 as lv2;

/// Native plugin editor-window hosting. Enable with the `editor` feature.
///
/// This re-exports `notwig-rack-editor`, which owns the outer native window,
/// editor lifecycle, parameter polling, and PNG capture for any
/// [`core::Plugin`].
#[cfg(feature = "editor")]
pub use notwig_rack_editor as editor_window;

/// ARA (Audio Random Access) host support. Enable with the `ara` feature.
///
/// This re-exports `notwig-rack-ara`. The individual format wrappers expose
/// their companion bindings through the `ara` feature on each crate.
#[cfg(feature = "ara")]
pub use notwig_rack_ara as ara;

/// Audio Unit v2 host (Apple platforms). Enable with the `au` feature.
#[cfg(all(feature = "au", target_vendor = "apple"))]
pub use notwig_rack_au as au;

/// Audio Unit v3 host (Apple platforms). Enable with the `au3` feature.
#[cfg(all(feature = "au3", target_vendor = "apple"))]
pub use notwig_rack_au3 as au3;
