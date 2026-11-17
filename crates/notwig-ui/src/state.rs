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
            pixels_per_beat: 64.0,
            scroll_track: 0,
        }
    }
}

impl Viewport {
    pub const MIN_PPB: f32 = 4.0;
    pub const MAX_PPB: f32 = 800.0;

    #[must_use]
    pub fn beat_to_x(self, beat: TimeMus, timeline_left: f32) -> f32 {
        timeline_left + ((beat.as_f64() - self.start.as_f64()) as f32) * self.pixels_per_beat
    }

    #[must_use]
    pub fn x_to_beat(self, x: f32, timeline_left: f32) -> TimeMus {
        let beats = f64::from(x - timeline_left) / f64::from(self.pixels_per_beat.max(0.001))
            + self.start.as_f64();
        TimeMus::new((beats.max(0.0) * 1_000_000.0).round() as i64, 1_000_000)
    }

    #[must_use]
    pub fn visible_beats(self, width: f32) -> f64 {
        f64::from((width / self.pixels_per_beat.max(0.001)).max(1.0))
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.pixels_per_beat = (self.pixels_per_beat * factor).clamp(Self::MIN_PPB, Self::MAX_PPB);
    }

    pub fn zoom_at(&mut self, pointer_x: f32, timeline_left: f32, factor: f32) {
        let ppb = self.pixels_per_beat.clamp(Self::MIN_PPB, Self::MAX_PPB);
        let beat_under = self.start.as_f64() + f64::from((pointer_x - timeline_left) / ppb);
        let new_ppb = (ppb * factor).clamp(Self::MIN_PPB, Self::MAX_PPB);
        let new_start = (beat_under - f64::from((pointer_x - timeline_left) / new_ppb)).max(0.0);
        self.pixels_per_beat = new_ppb;
        self.start = TimeMus::new((new_start * 1_000_000.0).round() as i64, 1_000_000);
    }

    pub fn pan_beats(&mut self, delta: f64) {
        let start = (self.start.as_f64() + delta).max(0.0);
        self.start = TimeMus::new((start * 1_000_000.0).round() as i64, 1_000_000);
    }

    #[must_use]
    pub fn scroll_start(self, content_end: f64, visible_beats: f64) -> f64 {
        self.start
            .as_f64()
            .clamp(0.0, (content_end - visible_beats).max(0.0))
    }

    #[must_use]
    pub fn scroll_thumb(
        self,
        content_end: f64,
        visible_beats: f64,
        track_width: f32,
    ) -> (f32, f32) {
        let content = content_end.max(visible_beats).max(1.0);
        let start = self.scroll_start(content_end, visible_beats);
        let thumb_x = (start / content) as f32 * track_width;
        let thumb_w = ((visible_beats / content) as f32 * track_width)
            .max(24.0)
            .min(track_width);
        (thumb_x, thumb_w)
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiState {
    pub selection: Selection,
    pub viewport: Viewport,
    pub notifications: Vec<Notification>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_roundtrips_beats() {
        let viewport = Viewport::default();
        let beat = TimeMus::new(4, 1);
        let x = viewport.beat_to_x(beat, 230.0);
        assert!((viewport.x_to_beat(x, 230.0) - beat).as_f64().abs() < 1e-6);
    }
}
