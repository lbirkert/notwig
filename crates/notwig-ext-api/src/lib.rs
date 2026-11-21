#![deny(unsafe_code)]

pub mod abi;
pub mod config;
pub mod context;
pub mod draw;
pub mod extension;
pub mod geometry;
pub mod index;
pub mod manifest;
pub mod note;
pub mod panel;
pub mod status;
pub mod target;
pub mod ui;

pub use abi::{ABI_VERSION, AbiStatus, pack_ptr_len, unpack_ptr_len};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, DeviceMeterSnapshot, ThemeSnapshot};
pub use draw::{
    DrawCommand, DrawError, DrawList, FontFamily, MAX_ACTIONS, MAX_COMMANDS, MAX_OUTPUT_BYTES,
    MAX_PATH_POINTS, MAX_TEXT_BYTES, VerticalAlign,
};
pub use extension::{DeviceExtension, Extension};
pub use geometry::{Color, Point, Rect, Size, Stroke, TextAlign};
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl};
pub use manifest::{
    ExtensionCapability, ExtensionDeviceDecl, ExtensionEntryPoint, ExtensionManifest, ManifestError,
};
pub use note::{NOTE_EVENT_BYTES, NoteEvent, NoteKind, decode_note_event, encode_note_event};
pub use notwig_ext_macros::notwig_extension;
pub use panel::{DevicePanelHost, PanelAction, PanelCommand, PanelDraw};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus};
pub use ui::{
    Align, CursorIcon, Drawer, Font, Modifiers, PointerState, RecordDrawer, RecordUi, Response,
    Sense, Style, UiBackend, UiContext, UiEvent, UiId, UiInput, UiMemory,
};
pub use ui::{
    METER_MIN_DB, button, divider, dropdown, knob, label, paint_button, paint_divider,
    paint_dropdown, paint_knob, paint_label, paint_section, paint_stereo_meter, paint_toggle,
    section, stereo_meter, toggle,
};
