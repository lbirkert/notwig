#![deny(unsafe_code)]

pub mod abi;
pub mod config;
pub mod context;
pub mod draw;
pub mod extension;
pub mod geometry;
pub mod index;
pub mod manifest;
pub mod panel;
pub mod status;

pub use abi::{ABI_VERSION, AbiStatus, pack_ptr_len, unpack_ptr_len};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, ThemeSnapshot};
pub use draw::{
    DrawCommand, DrawError, DrawList, MAX_COMMANDS, MAX_OUTPUT_BYTES, MAX_PATH_POINTS,
    MAX_TEXT_BYTES,
};
pub use extension::{DeviceExtension, Extension};
pub use geometry::{Color, Point, Rect, Size, Stroke, TextAlign};
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry};
pub use manifest::{ExtensionCapability, ExtensionDeviceDecl, ExtensionManifest, ManifestError};
pub use notwig_ext_macros::notwig_extension;
pub use panel::{DevicePanelHost, PanelCommand, PanelDraw};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus, TrustLevel};
