//! Host-side discovery, loading, and integration of Notwig extensions.
//!
//! [`ExtensionHost`] scans an extensions directory, validates installed
//! manifests, loads WASM modules through `wasmtime` or native shared libraries,
//! and exposes device factories and panel hosts to the rest of the app. The
//! crate also provides archive validation, ZIP install/uninstall helpers, and
//! the browser-specific `WebExtensionHost` for WASM-only packages.
//!
//! ```text
//! let host = notwig_ext::ExtensionHost::load(extensions_dir, &settings)?;
//! let specs = host.specs();
//! let factory = host.factory();
//! let panel_host = host.panel_host();
//! ```

#![deny(unsafe_code)]

mod archive;
mod error;

pub use archive::{
    ExtensionArchive, MAX_ARCHIVE_BYTES, read_archive, read_extension_files, validate_extension_id,
};
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
mod instance;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
mod panel;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;

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
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use web::{WebExtensionHost, WebExtensionPackage};

/// Alias used by host integrations for [`ExtensionError`].
pub type HostError = ExtensionError;
