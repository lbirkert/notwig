//! CLAP host implementation for the notwig-rack framework.
//!
//! CLAP is the simplest format to host because the entire API is
//! pure C with no platform-specific oddities. notwig-rack-clap uses
//! `clap-sys` for raw bindings and `libloading` to open plugin
//! bundles at runtime — no C++ glue is involved.
//!
//! # Lifecycle
//!
//! Each `.clap` bundle ships a single `clap_entry` symbol. The
//! scanner opens the bundle, calls `entry.init(path)`, asks the
//! factory at `CLAP_PLUGIN_FACTORY_ID` to enumerate its plugins,
//! converts each `clap_plugin_descriptor` into a
//! [`PluginInfo`](notwig_rack_core::info::PluginInfo)
//! and then calls `entry.deinit()` to release scanner-only state.
//!
//! [`ClapScanner`]`::load` re-opens the bundle, holds the entry +
//! library alive for the lifetime of the returned [`ClapPlugin`],
//! and on `Drop` calls the plugin's `destroy` followed by the
//! entry's `deinit`.

#[cfg(feature = "ara")]
mod ara;
mod editor;
mod events;
mod host;
mod library;
mod plugin;
mod process;
mod transport;

pub use library::{ClapScanner, default_clap_paths};
pub use plugin::ClapPlugin;

/// Format identifier — used as the `format` field on
/// [`PluginInfo`](notwig_rack_core::info::PluginInfo) entries this
/// crate returns.
pub const FORMAT: &str = "clap";

/// Filename extension every CLAP plugin uses, including the
/// leading dot.
pub const CLAP_EXTENSION: &str = ".clap";
