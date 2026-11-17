#![forbid(unsafe_code)]

mod projection;
mod settings;
mod state;

pub use projection::{
    ArrangerRow, ArrangerView, ClipView, DeviceSlot, MixerStrip, MixerView, ProjectionError,
    SceneSummary, project_arranger, project_devices, project_mixer, scene_summary,
};
pub use settings::{AudioSettingsSnapshot, SettingsIntent, SettingsSnapshot};
pub use state::{Notification, NotificationKind, PanelSizes, Selection, UiState, ViewId, Viewport};
