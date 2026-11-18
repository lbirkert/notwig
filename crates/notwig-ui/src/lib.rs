#![forbid(unsafe_code)]

mod arranger;
mod device;
mod drop_target;
mod edit;
mod form;
mod icons;
mod info;
mod intent;
mod panel;
mod render;
mod runtime;
mod settings;
mod shell;
mod state;
mod summary;
mod theme;
mod timeline;
mod widgets;

pub use arranger::ClipDrag;
pub use edit::{clamp_node_shift, paste_target};
pub use icons::Icon;
pub use intent::{DropAudioTarget, UiIntent};
pub use panel::clip_editor::ClipEditorState;
pub use runtime::{
    AudioDeviceView, AudioDriverView, AudioSettingsValue, AudioSettingsView, MeterValues,
    PluginEntry, RuntimeView,
};
pub use shell::NotwigUi;
pub use state::{
    EditFocus, Notification, NotificationKind, OpenedEditor, Selection, UiState, Viewport,
};
pub use summary::{SceneSummary, scene_summary};
pub use widgets::{
    format_beats, format_db, format_param, snap_16, snap_label, snap_step, snap_time,
};

#[doc(hidden)]
pub mod test_api {
    pub use crate::drop_target::test_api as drop_target;
    pub use crate::panel::clip_editor::test_api as clip_editor;
    pub use crate::shell::test_api as shell;
    pub use crate::timeline::test_api as timeline;
    pub use crate::widgets::test_api as widgets;
}
