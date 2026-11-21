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
pub use host::{HostAdapter, PluginHost, UnavailableAdapter};
#[cfg(any(
    feature = "vst3",
    feature = "vst2",
    feature = "au",
    feature = "au3",
    feature = "clap",
    feature = "lv2"
))]
pub use native_editor::NativePluginWindow;
pub use types::{
    PluginCapabilities, PluginDescriptor, PluginError, PluginFormat, PluginInstance, PluginState,
};
