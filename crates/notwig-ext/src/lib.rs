#![forbid(unsafe_code)]

mod error;

pub use error::ExtensionError;

#[cfg(feature = "native")]
mod abi;
#[cfg(feature = "native")]
mod compiled;
#[cfg(feature = "native")]
mod device;
#[cfg(feature = "native")]
mod discovery;
#[cfg(feature = "native")]
mod host;
#[cfg(feature = "native")]
mod install;
#[cfg(feature = "native")]
mod panel;

#[cfg(feature = "native")]
pub use device::ExtensionDeviceFactory;
#[cfg(feature = "native")]
pub use discovery::{DiscoveredExtension, discover};
#[cfg(feature = "native")]
pub use host::ExtensionHost;
#[cfg(feature = "native")]
pub use install::{InstalledExtension, install_zip_bytes, install_zip_file, uninstall};
#[cfg(feature = "native")]
pub use panel::ExtensionPanelHost;

pub type HostError = ExtensionError;
