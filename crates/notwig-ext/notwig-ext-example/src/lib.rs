//! Example Notwig extension.
//!
//! Implementations live in modules. This crate root only wires exports and
//! keeps lifecycle entrypoints last.

mod example_panel;
mod humanize;
mod pan;
mod point;

pub use example_panel::{
    EXAMPLE_PANEL_ID, ExampleBottomPanel, OPEN_EXAMPLE_PANEL_COMMAND_ID, OpenExamplePanelCommand,
};
pub use humanize::{HUMANIZE_COMMAND_ID, HumanizeCommand};
pub use pan::{PAN_SUB_EDITOR_ID, PanSubEditor};
pub use point::{EXAMPLE_POINT, ExamplePoint};

use notwig_ext_api::{
    EP_COMMANDS, EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR, EP_NOTWIG_BOTTOM_PANEL, ExtensionBuilder,
    PreloadBuilder, UnloadContext, notwig_ext,
};

#[notwig_ext(preload)]
pub fn preload(extension: &mut PreloadBuilder) {
    extension.register_extension_point(EXAMPLE_POINT);
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<HumanizeCommand>(EP_COMMANDS);
    extension.register_extension::<OpenExamplePanelCommand>(EP_COMMANDS);
    extension.register_extension::<PanSubEditor>(EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR);
    extension.register_extension::<ExampleBottomPanel>(EP_NOTWIG_BOTTOM_PANEL);
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {}
