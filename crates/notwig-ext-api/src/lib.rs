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
pub mod ui;

pub use abi::{ABI_VERSION, AbiStatus, pack_ptr_len, unpack_ptr_len};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, ThemeSnapshot};
pub use draw::{
    DrawCommand, DrawError, DrawList, FontFamily, MAX_ACTIONS, MAX_COMMANDS, MAX_OUTPUT_BYTES,
    MAX_PATH_POINTS, MAX_TEXT_BYTES, VerticalAlign,
};
pub use extension::{DeviceExtension, Extension};
pub use geometry::{Color, Point, Rect, Size, Stroke, TextAlign};
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry};
pub use manifest::{ExtensionCapability, ExtensionDeviceDecl, ExtensionManifest, ManifestError};
pub use notwig_ext_macros::notwig_extension;
pub use panel::{DevicePanelHost, PanelAction, PanelCommand, PanelDraw};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus, TrustLevel};
pub use ui::{
    Align, CursorIcon, Drawer, Font, Modifiers, PointerState, RecordDrawer, RecordUi, Response,
    Sense, Style, UiBackend, UiContext, UiEvent, UiId, UiInput, UiMemory,
};
pub use ui::{
    button, divider, knob, label, paint_button, paint_divider, paint_knob, paint_label,
    paint_section, paint_toggle, section, toggle,
};
