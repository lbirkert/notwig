#![forbid(unsafe_code)]

mod abi;
mod compiled;
mod device;
mod discovery;
mod error;
mod host;
mod install;
mod panel;

pub use device::ExtensionDeviceFactory;
pub use discovery::{DiscoveredExtension, discover};
pub use error::ExtensionError;
pub use host::ExtensionHost;
pub use install::{InstalledExtension, install_zip_bytes, install_zip_file, uninstall};
pub use panel::ExtensionPanelHost;

pub type HostError = ExtensionError;
