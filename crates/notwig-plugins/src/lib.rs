#[cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "au",
    feature = "au3",
    feature = "clap",
    feature = "lv2"
))]
mod adapters;
mod cache;
mod host;
#[cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "au",
    feature = "au3",
    feature = "clap",
    feature = "lv2"
))]
mod native_editor;
#[cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "au",
    feature = "au3",
    feature = "clap",
    feature = "lv2"
))]
mod rack;
mod types;

pub use cache::{PluginCache, ScanRecord};
pub use host::{HostAdapter, PluginHost, PluginProbe, UnavailableAdapter};
#[cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "au",
    feature = "au3",
    feature = "clap",
    feature = "lv2"
))]
pub use native_editor::{NativePluginWindow, prepare_window_host, pump_window_host};
pub use types::{
    PLUGIN_METADATA_PREFIX, PluginCapabilities, PluginDescriptor, PluginError, PluginFormat,
    PluginInstance, PluginMetadata, PluginState,
};
