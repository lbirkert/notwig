//! Author-facing SDK for building Notwig extensions.
//!
//! An extension implements [`DeviceExtension`] for each device type,
//! [`CommandExtension`] for commands, and [`AudioSubEditorExtension`] for
//! extension-drawn audio sub-editors. It registers implementations during a
//! [`ExtensionBuilder`] load phase, optionally declares extension points in a
//! [`PreloadBuilder`] phase, and releases resources in an [`UnloadContext`]
//! phase. Each phase is annotated with `#[notwig_ext(preload)]`,
//! `#[notwig_ext(load)]`, or `#[notwig_ext(unload)]`.
//!
//! ```text
//! use notwig_ext_api::{EP_NOTWIG_DEVICES, DeviceExtension, ExtensionBuilder, notwig_ext};
//!
//! struct MyDevice;
//! impl DeviceExtension for MyDevice {
//!     fn new() -> Self { Self }
//!     fn spec() -> notwig_core::dsp::DeviceSpec { /* build the spec */ }
//!     fn draw(&mut self, ui: &mut notwig_ext_api::egui::Ui, meter: notwig_ext_api::DeviceMeterSnapshot) { /* draw */ }
//!     fn process(&mut self, samples: &mut [f32]) { /* in-place DSP */ }
//! }
//!
//! #[notwig_ext(load)]
//! pub fn load(extension: &mut ExtensionBuilder) {
//!     extension.register_extension::<MyDevice>(EP_NOTWIG_DEVICES);
//! }
//! ```
//!
//! The host requires an exact [`ABI_VERSION`] match. Changing an export
//! signature, wire layout, or documented contract requires bumping that value.

#![deny(unsafe_code)]

pub mod abi;
pub mod audio_editor;
pub mod bottom_panel;
pub mod clipboard;
pub mod command;
pub mod config;
pub mod context;
pub mod cursor;
pub mod panel_frame;
pub use egui;
pub mod extension;
pub mod extension_point;
pub mod geometry;
pub mod index;
pub mod lifecycle;
pub mod manifest;
pub mod note;
pub mod panel;
pub mod registration;
pub mod registry;
pub mod status;
pub mod target;

pub use abi::{ABI_VERSION, AbiStatus, MAX_OUTPUT_BYTES, pack_ptr_len, unpack_ptr_len};
pub use audio_editor::{
    AUDIO_SUB_EDITOR_CONTRACT, AUDIO_SUB_EDITOR_POINT, AudioSubEditorClip, AudioSubEditorContext,
    AudioSubEditorDescriptor, AudioSubEditorExtension, AudioSubEditorFrame, AudioSubEditorHost,
    AudioSubEditorInterface, AudioSubEditorResponse, AudioSubEditorSelectionActions,
    AudioWaveformColumn, AudioWaveformPreview, CurveTarget, EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR,
    MAX_AUDIO_SUB_EDITOR_CLIPBOARD_POINTS, MAX_AUDIO_SUB_EDITOR_TITLE_BYTES,
};
pub use bottom_panel::{
    BottomPanelContext, BottomPanelDescriptor, BottomPanelExtension, BottomPanelFrame,
    BottomPanelHost, BottomPanelInterface, BottomPanelResponse, EP_NOTWIG_BOTTOM_PANEL,
    MAX_BOTTOM_PANEL_TITLE_BYTES,
};
pub use clipboard::{ClipboardProvider, EP_NOTWIG_CLIPBOARD};
pub use command::{
    COMMAND_CONTRACT, COMMAND_POINT, CommandContext, CommandDescriptor, CommandEdit, CommandEffect,
    CommandExtension, CommandFocus, CommandId, CommandInterface, CommandSelection,
    CommandTimeSelection, CommandUiRequest, EP_COMMANDS, MAX_COMMAND_CONTEXT_NOTES,
    MAX_COMMAND_CONTEXT_SLICES, MAX_COMMAND_DESCRIPTION_BYTES, MAX_COMMAND_EDITS, WhenError,
    evaluate_when, validate_when,
};
pub use config::{CONFIG_VERSION, DEFAULT_INDEX_URL, ExtensionConfig};
pub use context::{DeviceDrawContext, DeviceMeterSnapshot, ThemeSnapshot};
pub use cursor::CursorIcon;
pub use extension::{DeviceExtension, DeviceInterface, EP_NOTWIG_DEVICES, Extension};
pub use extension_point::{Cardinality, ExtensionPoint};
pub use geometry::Color;
pub use index::{ExtensionIndexDevice, ExtensionIndexEntry, ExtensionIndexUrl};
pub use lifecycle::{ExtensionBuilder, ExtensionPointFor, PreloadBuilder, UnloadContext};
pub use manifest::{
    CURRENT_MANIFEST_VERSION, ExtensionCapability, ExtensionContribution, ExtensionDeviceDecl,
    ExtensionEntryPoint, ExtensionInclude, ExtensionManifest, ManifestError,
};
pub use note::{NOTE_EVENT_BYTES, NoteEvent, NoteKind, decode_note_event, encode_note_event};
pub use notwig_ext_macros::{notwig_ext, notwig_extension};
pub use panel::{DevicePanelHost, PanelAction};
pub use panel_frame::{
    MAX_PANEL_FRAME_BYTES, PanelEvent, PanelFrame, PanelImage, PanelInput, PanelMesh,
    PanelModifiers, PanelPointerButton, PanelPrimitive, PanelTextureDelta, PanelTextureOptions,
    PanelVertex,
};
pub use registration::{
    CORE_EXTENSION_ID, CORE_EXTENSION_NAME, DeviceRegistrationDescriptor, ExtensionPointDescriptor,
    ExtensionRegistrations, REGISTRATION_MANIFEST_VERSION,
};
pub use registry::{CoreExtensionRegistry, RegistryError};
pub use status::{ExtensionInfo, ExtensionSettings, ExtensionStatus};
