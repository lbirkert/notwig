#![forbid(unsafe_code)]

mod arranger;
mod device;
mod intent;
mod projection;
mod runtime;
mod settings;
mod shell;
mod state;
mod theme;
mod widgets;

pub use arranger::ClipDrag;
pub use intent::UiIntent;
pub use projection::{
    ArrangerRow, ArrangerView, ClipView, DeviceSlot, MixerStrip, MixerView, ProjectionError,
    SceneSummary, project_arranger, project_devices, project_mixer, scene_summary,
};
pub use runtime::{MeterValues, PluginEntry, RuntimeView};
pub use settings::{AudioSettingsSnapshot, SettingsIntent, SettingsSnapshot};
pub use shell::NotwigUi;
pub use state::{Notification, NotificationKind, PanelSizes, Selection, UiState, Viewport};
pub use widgets::{format_beats, format_db, format_param, snap_16};
