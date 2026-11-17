#![forbid(unsafe_code)]

mod cache;
mod host;
mod types;

pub use cache::{PluginCache, ScanRecord};
pub use host::{HostAdapter, PluginHost, UnavailableAdapter};
pub use types::{
    PluginCapabilities, PluginDescriptor, PluginError, PluginFormat, PluginInstance, PluginState,
};
