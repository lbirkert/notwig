mod bindings;
mod catalog;
mod chord;
mod registry;

pub use bindings::Shortcuts;
#[allow(unused_imports)]
pub use catalog::CommandSpec;
pub use catalog::{COMMANDS, CommandCategory, ShortcutCommand};
pub use chord::KeyChord;
pub use registry::CommandRegistry;

#[derive(Debug, Default)]
pub(crate) struct ShortcutEditorState {
    pub(crate) capture: Option<String>,
    pub(crate) filter: String,
}
