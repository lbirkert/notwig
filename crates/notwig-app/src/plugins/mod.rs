#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{PluginCatalog, new_plugin_host, plugin_entries};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{PluginCatalog, new_plugin_host, plugin_entries};
