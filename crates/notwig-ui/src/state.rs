use std::collections::BTreeSet;

use notwig_core::{ClipId, DeviceId, NoteId, TimeMus, TrackId};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub tracks: BTreeSet<TrackId>,
    pub clips: BTreeSet<ClipId>,
    pub notes: BTreeSet<NoteId>,
    pub devices: BTreeSet<DeviceId>,
    pub primary_track: Option<TrackId>,
}

impl Selection {
    pub fn toggle_track(&mut self, id: TrackId) {
        if !self.tracks.remove(&id) {
            self.tracks.insert(id);
            self.primary_track = Some(id);
        } else if self.primary_track == Some(id) {
            self.primary_track = self.tracks.iter().next().copied();
        }
    }

    pub fn select_only_track(&mut self, id: TrackId) {
        self.tracks.clear();
        self.tracks.insert(id);
        self.primary_track = Some(id);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub start: TimeMus,
    pub pixels_per_beat: f32,
    pub scroll_track: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            start: TimeMus::zero(),
            pixels_per_beat: 32.0,
            scroll_track: 0,
        }
    }
}

impl Viewport {
    #[must_use]
    pub fn beat_to_x(self, beat: TimeMus) -> f32 {
        ((beat.as_f64() - self.start.as_f64()) as f32) * self.pixels_per_beat
    }

    #[must_use]
    pub fn x_to_beat(self, x: f32) -> TimeMus {
        let beats = f64::from(x) / f64::from(self.pixels_per_beat.max(0.001)) + self.start.as_f64();
        TimeMus::new((beats * 1_000_000.0).round() as i64, 1_000_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewId {
    Arranger,
    Mixer,
    Device,
    Browser,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelSizes {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Default for PanelSizes {
    fn default() -> Self {
        Self {
            left: 240.0,
            right: 300.0,
            bottom: 180.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub kind: NotificationKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiState {
    pub selection: Selection,
    pub viewport: Viewport,
    pub panels: PanelSizes,
    pub active_view: ViewId,
    pub notifications: Vec<Notification>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            viewport: Viewport::default(),
            panels: PanelSizes::default(),
            active_view: ViewId::Arranger,
            notifications: Vec::new(),
        }
    }
}

impl UiState {
    pub fn notify(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notifications.push(Notification {
            kind,
            message: message.into(),
        });
        if self.notifications.len() > 64 {
            self.notifications.remove(0);
        }
    }
}
