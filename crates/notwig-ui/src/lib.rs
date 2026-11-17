#![forbid(unsafe_code)]

mod arranger;
mod clip_editor;
mod device;
mod drop_target;
mod edit;
mod icons;
mod info;
mod intent;
mod runtime;
mod shell;
mod state;
mod summary;
mod theme;
mod timeline;
mod widgets;

pub use arranger::ClipDrag;
pub use clip_editor::ClipEditorState;
pub use edit::{
    ClipboardMode, DuplicateSpan, EditTarget, bar_duplicate_target, clamp_bar_shift,
    clamp_node_shift, duplicate_bar_range, duplicate_plan, node_duplicate_target, paste_target,
};
pub use icons::Icon;
pub use intent::{ClipDuplicate, DropAudioTarget, UiIntent};
pub use runtime::{MeterValues, PluginEntry, RuntimeView};
pub use shell::NotwigUi;
pub use state::{EditFocus, Notification, NotificationKind, Selection, UiState, Viewport};
pub use summary::{SceneSummary, scene_summary};
pub use widgets::{
    format_beats, format_db, format_param, snap_16, snap_label, snap_step, snap_time,
};
