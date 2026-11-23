#![forbid(unsafe_code)]

pub mod audio;
pub mod audio_config;
mod audio_input;
#[cfg(not(target_arch = "wasm32"))]
mod audio_worker;
pub mod clock;
pub mod edit_lowering;
mod engine_build;
#[cfg(not(target_arch = "wasm32"))]
pub mod engine_process;
mod extensions;
pub mod gui;
pub mod history;
pub mod logging;
#[cfg(not(target_arch = "wasm32"))]
pub mod loopback;
mod plugins;
mod recording;
mod shortcuts;

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub fn probe_plugin_metadata(
    descriptor: &notwig_plugins::PluginDescriptor,
) -> Result<notwig_plugins::PluginMetadata, notwig_plugins::PluginError> {
    plugins::new_plugin_host().probe_metadata(descriptor)
}
#[cfg(target_arch = "wasm32")]
pub mod web;
pub mod workspace;
