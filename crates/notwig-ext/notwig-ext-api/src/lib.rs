#![deny(unsafe_code)]

pub mod abi;
pub mod config;
pub mod context;
pub mod cursor;
pub mod panel_frame;
pub use egui;
pub mod extension;
pub mod geometry;
pub mod index;
pub mod manifest;
pub mod note;
pub mod panel;
pub mod status;
pub mod target;

pub use abi::{ABI_VERSION, AbiStatus, MAX_OUTPUT_BYTES, pack_ptr_len, unpack_ptr_len};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, DeviceMeterSnapshot, ThemeSnapshot};
pub use cursor::CursorIcon;
pub use extension::{DeviceExtension, Extension};
pub use geometry::Color;
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl};
pub use manifest::{
    ExtensionCapability, ExtensionDeviceDecl, ExtensionEntryPoint, ExtensionInclude,
    ExtensionManifest, ManifestError,
};
pub use note::{NOTE_EVENT_BYTES, NoteEvent, NoteKind, decode_note_event, encode_note_event};
pub use notwig_ext_macros::notwig_extension;
pub use panel::{DevicePanelHost, PanelAction};
pub use panel_frame::{
    MAX_PANEL_FRAME_BYTES, PanelEvent, PanelFrame, PanelImage, PanelInput, PanelMesh,
    PanelModifiers, PanelPointerButton, PanelPrimitive, PanelTextureDelta, PanelTextureOptions,
    PanelVertex,
};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus};
