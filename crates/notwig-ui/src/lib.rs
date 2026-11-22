#![forbid(unsafe_code)]

mod clipboard;
mod commands;
mod intent;
mod runtime;
mod state;
mod views;
mod waveform;
mod widgets;

pub(crate) use views::{arranger, settings, shell};
pub(crate) use widgets::{drop_target, form, icons, theme, timeline};

pub use clipboard::Clipboard;
pub use commands::{KeyChord, ShortcutCommand, Shortcuts};
pub use intent::{DropAudioTarget, ExportBitDepth, ExportFormat, ExportOptions, UiIntent};
pub use runtime::{
    AudioDeviceView, AudioDriverView, AudioSettingsValue, AudioSettingsView, DspAnalysisView,
    MeterValues, PluginEntry, ProjectPickerAction, ProjectPickerView, RecentProjectView,
    RuntimeView,
};
pub use shell::NotwigUi;

/// Installs the shared theme and fonts before the first frame is painted.
pub fn configure(ctx: &egui::Context) {
    theme::apply(ctx);
}
pub use state::{
    EditFocus, Notification, NotificationKind, OpenedEditor, Selection, UiState, Viewport,
};
pub use views::arranger::ClipDrag;
pub use views::arranger::edit::{clamp_node_shift, paste_target};
pub use views::audio_editor::{ClipEditorState, ClipLane};
pub use views::midi_editor::MidiEditorState;
pub use views::settings::SettingsCategory;
pub use waveform::{AudioWaveform, WaveformCache};
pub use widgets::icons::Icon;
pub use widgets::{
    SNAP_LEVELS, SnapLevel, SnapSettings, adaptive_step, format_beats, format_db, format_param,
    snap_16, snap_label, snap_step, snap_step_label, snap_time,
};

#[doc(hidden)]
pub mod test_api {
    pub use crate::shell::test_api as shell;
    pub use crate::views::audio_editor::test_api as audio_editor;
    pub use crate::views::device_editor::test_api as device_editor;
    pub use crate::views::midi_editor::test_api as midi_editor;
    pub use crate::waveform::test_api as waveform;
    pub use crate::widgets::drop_target::test_api as drop_target;
    pub use crate::widgets::test_api as widgets;
    pub use crate::widgets::timeline::test_api as timeline;
}
