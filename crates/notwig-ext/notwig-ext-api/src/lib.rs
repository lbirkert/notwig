//! Author-facing SDK for building Notwig extensions.
//!
//! An extension implements [`DeviceExtension`] for each device type,
//! [`CommandExtension`] for commands, and [`AudioSubEditorExtension`] for
//! extension-drawn audio sub-editors. It registers implementations on an
//! [`Extension`] runtime and exposes one entrypoint annotated with
//! [`notwig_extension`]. The macro generates the WASM or native ABI exports
//! that the host uses to describe, create, prepare, process, draw, execute, and
//! render contributions. Panels use the re-exported [`egui`] API and return a
//! validated [`panel_frame::PanelFrame`].
//!
//! ```text
//! use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};
//!
//! struct MyDevice;
//! impl DeviceExtension for MyDevice {
//!     fn new() -> Self { Self }
//!     fn spec() -> notwig_core::dsp::DeviceSpec { /* build the spec */ }
//!     fn draw(&mut self, ui: &mut notwig_ext_api::egui::Ui, meter: notwig_ext_api::DeviceMeterSnapshot) { /* draw */ }
//!     fn process(&mut self, samples: &mut [f32]) { /* in-place DSP */ }
//! }
//!
//! #[notwig_extension]
//! pub fn extension(extension: &mut Extension) {
//!     extension.register_device::<MyDevice>();
//! }
//! ```
//!
//! The host requires an exact [`ABI_VERSION`] match. Changing an export
//! signature, wire layout, or documented contract requires bumping that value.

#![deny(unsafe_code)]

pub mod abi;
pub mod audio_editor;
pub mod command;
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
pub use audio_editor::{
    AUDIO_SUB_EDITOR_CONTRACT, AUDIO_SUB_EDITOR_POINT, AudioSubEditorClip, AudioSubEditorContext,
    AudioSubEditorDescriptor, AudioSubEditorExtension, AudioSubEditorFrame, AudioSubEditorHost,
    AudioSubEditorResponse, AudioSubEditorSelectionActions, AudioWaveformColumn,
    AudioWaveformPreview, CurveTarget, MAX_AUDIO_SUB_EDITOR_CLIPBOARD_POINTS,
    MAX_AUDIO_SUB_EDITOR_TITLE_BYTES,
};
pub use command::{
    COMMAND_CONTRACT, COMMAND_POINT, CommandContext, CommandDescriptor, CommandEdit, CommandEffect,
    CommandExtension, CommandFocus, CommandId, CommandSelection, CommandTimeSelection,
    CommandUiRequest, MAX_COMMAND_CONTEXT_NOTES, MAX_COMMAND_CONTEXT_SLICES, MAX_COMMAND_EDITS,
    WhenError, evaluate_when, validate_when,
};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, DeviceMeterSnapshot, ThemeSnapshot};
pub use cursor::CursorIcon;
pub use extension::{DeviceExtension, Extension};
pub use geometry::Color;
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl};
pub use manifest::{
    CURRENT_MANIFEST_VERSION, ExtensionCapability, ExtensionContribution, ExtensionDeviceDecl,
    ExtensionEntryPoint, ExtensionInclude, ExtensionManifest, ManifestError,
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
