#![forbid(unsafe_code)]

#[cfg(feature = "app")]
mod clipboard;
#[cfg(feature = "app")]
mod commands;
#[cfg(any(feature = "app", feature = "extension-ui"))]
pub mod extension_ui;
#[cfg(feature = "app")]
mod intent;
#[cfg(feature = "app")]
mod runtime;
#[cfg(feature = "app")]
mod state;
#[cfg(feature = "app")]
mod views;
#[cfg(feature = "app")]
mod waveform;
#[cfg(any(feature = "app", feature = "extension-ui"))]
mod widgets;

pub mod components;
mod fine_drag;
pub mod palette;

pub use components::{
    DB_MAX, DB_MIN, button, divider, draw_stereo_meter, draw_stereo_meter_with_peak, dropdown,
    format_db, format_param, knob, label, section, toggle, value_drag, volume_fader,
};

#[cfg(feature = "app")]
pub(crate) use views::{arranger, settings, shell};
#[cfg(feature = "app")]
pub(crate) use widgets::{drop_target, form, icons, theme, timeline};

#[cfg(feature = "app")]
pub use clipboard::Clipboard;
#[cfg(feature = "app")]
pub use commands::{CommandRegistry, KeyChord, ShortcutCommand, Shortcuts};
#[cfg(feature = "app")]
pub use intent::{DropAudioTarget, ExportBitDepth, ExportFormat, ExportOptions, UiIntent};
#[cfg(feature = "app")]
pub use runtime::{
    AudioChannelConfig, AudioChannelDirection, AudioChannelFormat, AudioDeviceView,
    AudioDriverView, AudioInputView, AudioSettingsValue, AudioSettingsView, DspAnalysisView,
    MAX_AUDIO_BLOCK_SIZE, MAX_AUDIO_BLOCK_SIZE_WITHOUT_LOW_LATENCY, MeterValues, PluginEntry,
    ProjectPickerAction, ProjectPickerView, RecentProjectView, RecordingTakeView, RuntimeView,
};
#[cfg(feature = "app")]
pub use shell::NotwigUi;

#[cfg(feature = "app")]
/// Installs the shared theme and fonts before the first frame is painted.
pub fn configure(ctx: &egui::Context) {
    theme::apply(ctx);
}
#[cfg(feature = "app")]
pub use state::{
    EditFocus, Notification, NotificationKind, OpenedEditor, Selection, UiState, Viewport,
};
#[cfg(feature = "app")]
pub use views::arranger::ClipDrag;
#[cfg(feature = "app")]
pub use views::arranger::edit::{clamp_node_shift, paste_target};
#[cfg(feature = "app")]
pub use views::audio_editor::{AudioSubEditor, ClipEditorState, ClipLane};
#[cfg(feature = "app")]
pub use views::midi_editor::MidiEditorState;
#[cfg(feature = "app")]
pub use views::settings::SettingsCategory;
#[cfg(feature = "app")]
pub use waveform::{AudioWaveform, WaveformCache};
#[cfg(feature = "app")]
pub use widgets::icons::Icon;
#[cfg(feature = "app")]
pub use widgets::{
    SNAP_LEVELS, SnapLevel, SnapSettings, adaptive_step, format_beats, snap_16, snap_label,
    snap_step, snap_step_label, snap_time,
};

#[cfg(feature = "app")]
#[doc(hidden)]
pub mod test_api {
    pub use crate::fine_drag::test_api as fine_drag;
    pub use crate::shell::test_api as shell;
    pub use crate::views::audio_editor::test_api as audio_editor;
    pub use crate::views::device_editor::test_api as device_editor;
    pub use crate::views::midi_editor::test_api as midi_editor;
    pub use crate::views::settings::test_api as settings;
    pub use crate::waveform::test_api as waveform;
    pub use crate::widgets::drop_target::test_api as drop_target;
    pub use crate::widgets::line_editor::test_api as line_editor;
    pub use crate::widgets::test_api as widgets;
    pub use crate::widgets::timeline::test_api as timeline;
}
