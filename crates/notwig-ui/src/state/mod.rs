use notwig_core::TrackId;

use crate::widgets::SnapSettings;

mod notifications;
mod selection;
mod viewport;

pub use notifications::{Notification, NotificationKind};
pub use selection::{Selection, SelectionGroup, TimeSelection};
pub use viewport::Viewport;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditFocus {
    #[default]
    Timeline,
    ClipEditor,
    MidiEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenedEditor {
    DeviceEditor,
    ClipEditor,
    MidiEditor,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiState {
    pub selection: Selection,
    pub viewport: Viewport,
    pub snap: SnapSettings,
    pub notifications: Vec<Notification>,
    pub peak_hold: std::collections::BTreeMap<TrackId, f32>,
    /// Last timestamp passed to [`UiState::expire_notifications`]. Notifications
    /// created with [`UiState::notify`] use it as `created_at`, so the caller
    /// only has to provide a clock when expiring.
    clock: f64,
}

impl UiState {
    /// Records a block peak and returns the held maximum since the last reset.
    pub fn record_peak(&mut self, track_id: TrackId, peak: f32) -> f32 {
        let held = self.peak_hold.entry(track_id).or_insert(0.0);
        *held = held.max(peak.max(0.0));
        *held
    }

    pub fn reset_peak(&mut self, track_id: TrackId) {
        self.peak_hold.remove(&track_id);
    }
}
