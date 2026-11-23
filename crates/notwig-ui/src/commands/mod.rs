mod bindings;
mod catalog;
mod chord;

pub use bindings::Shortcuts;
#[allow(unused_imports)]
pub use catalog::CommandSpec;
pub use catalog::{COMMANDS, CommandCategory, ShortcutCommand};
pub use chord::KeyChord;

#[derive(Debug, Default)]
pub(crate) struct ShortcutEditorState {
    pub(crate) capture: Option<ShortcutCommand>,
    pub(crate) filter: String,
}
