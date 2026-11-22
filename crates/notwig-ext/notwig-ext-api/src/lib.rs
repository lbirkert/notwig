#![deny(unsafe_code)]

pub mod abi;
pub mod config;
pub mod context;
pub mod cursor;
pub mod egui_panel;
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
pub use egui_panel::{
    EguiEvent, EguiFrame, EguiImage, EguiInput, EguiMesh, EguiModifiers, EguiPointerButton,
    EguiPrimitive, EguiTextureDelta, EguiTextureOptions, EguiVertex, MAX_EGUI_FRAME_BYTES,
};
pub use extension::{DeviceExtension, Extension};
pub use geometry::Color;
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl};
pub use manifest::{
    ExtensionCapability, ExtensionDeviceDecl, ExtensionEntryPoint, ExtensionManifest, ManifestError,
};
pub use note::{NOTE_EVENT_BYTES, NoteEvent, NoteKind, decode_note_event, encode_note_event};
pub use notwig_ext_macros::notwig_extension;
pub use panel::{DevicePanelHost, PanelAction};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus};
