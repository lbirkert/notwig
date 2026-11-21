use std::collections::BTreeSet;

use notwig_core::{ClipId, DeviceId, NoteId, RangeMus, TimeMus, TrackId};

use crate::widgets::SnapSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionGroup<T: Ord> {
    items: BTreeSet<T>,
    primary: Option<T>,
}

impl<T: Ord + Copy> Default for SelectionGroup<T> {
    fn default() -> Self {
        Self {
            items: BTreeSet::new(),
            primary: None,
        }
    }
}

impl<T: Ord + Copy> SelectionGroup<T> {
    pub fn clear(&mut self) {
        self.items.clear();
        self.primary = None;
    }

    pub fn select_only(&mut self, item: T) {
        self.items.clear();
        self.items.insert(item);
        self.primary = Some(item);
    }

    pub fn toggle(&mut self, item: T) {
        if !self.items.remove(&item) {
            self.items.insert(item);
            self.primary = Some(item);
        } else if self.primary == Some(item) {
            self.primary = self.items.iter().next().copied();
        }
    }

    pub fn insert(&mut self, item: T) {
        self.items.insert(item);
        self.primary = Some(item);
    }

    pub fn contains(&self, item: &T) -> bool {
        self.items.contains(item)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn primary(&self) -> Option<T> {
        self.primary
    }

    pub fn set_primary(&mut self, item: T) {
        if self.items.contains(&item) {
            self.primary = Some(item);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.items.retain(|item| keep(item));
        if self
            .primary
            .is_some_and(|primary| !self.items.contains(&primary))
        {
            self.primary = self.items.iter().next().copied();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSelection {
    pub range: RangeMus,
    pub tracks: SelectionGroup<TrackId>,
}

impl TimeSelection {
    pub fn contains_track(&self, track: TrackId) -> bool {
        self.tracks.contains(&track)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub tracks: SelectionGroup<TrackId>,
    pub clips: SelectionGroup<ClipId>,
    pub notes: SelectionGroup<NoteId>,
    pub devices: SelectionGroup<DeviceId>,
    pub time: Option<TimeSelection>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.clips.clear();
        self.notes.clear();
        self.devices.clear();
        self.time = None;
    }

    pub fn select_time(&mut self, range: RangeMus, tracks: &[TrackId]) {
        self.clear();
        let mut group = SelectionGroup::default();
        for track in tracks {
            group.insert(*track);
            self.tracks.insert(*track);
        }
        self.time = Some(TimeSelection {
            range,
            tracks: group,
        });
    }

    pub fn select_track(&mut self, id: TrackId, additive: bool) {
        if additive {
            self.tracks.toggle(id);
        } else {
            self.clear();
            self.tracks.select_only(id);
        }
    }

    pub fn add_track(&mut self, id: TrackId) {
        self.clips.clear();
        self.notes.clear();
        self.devices.clear();
        self.time = None;
        self.tracks.insert(id);
    }

    pub fn select_track_range(&mut self, anchor: TrackId, target: TrackId, order: &[TrackId]) {
        let anchor_index = order.iter().position(|track| *track == anchor);
        let target_index = order.iter().position(|track| *track == target);
        let (Some(anchor_index), Some(target_index)) = (anchor_index, target_index) else {
            self.clear();
            self.tracks.select_only(target);
            return;
        };
        let (first, last) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        self.clear();
        for track in &order[first..=last] {
            self.tracks.insert(*track);
        }
        self.tracks.set_primary(target);
    }

    pub fn select_clip(&mut self, id: ClipId, track: TrackId, additive: bool) {
        if additive {
            self.clips.toggle(id);
            self.tracks.set_primary(track);
        } else {
            self.clear();
            self.clips.select_only(id);
            self.tracks.select_only(track);
        }
    }

    pub fn select_clips(&mut self, clips: &[(ClipId, TrackId)]) {
        self.clear();
        for (clip, track) in clips {
            self.clips.insert(*clip);
            self.tracks.insert(*track);
        }
    }

    pub fn select_device(&mut self, id: DeviceId, track: TrackId, additive: bool) {
        if additive {
            self.devices.toggle(id);
            self.tracks.set_primary(track);
        } else {
            self.clear();
            self.devices.select_only(id);
            self.tracks.select_only(track);
        }
    }

    pub fn select_only_track(&mut self, id: TrackId) {
        self.select_track(id, false);
    }

    pub fn toggle_track(&mut self, id: TrackId) {
        self.select_track(id, true);
    }

    pub fn primary_track(&self) -> Option<TrackId> {
        self.tracks.primary()
    }

    pub fn primary_device(&self) -> Option<DeviceId> {
        self.devices.primary()
    }

    pub fn primary_clip(&self) -> Option<ClipId> {
        self.clips.primary()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.clips.is_empty()
            && self.notes.is_empty()
            && self.devices.is_empty()
            && self.time.is_none()
    }

    #[must_use]
    pub fn to_edit_selection(&self) -> notwig_core::edit::Selection<TrackId, ClipId> {
        notwig_core::edit::Selection {
            objects: self.clips.iter().copied().collect(),
            time: self
                .time
                .as_ref()
                .map(|time| notwig_core::edit::TimeSelection {
                    span: time.range,
                    lanes: time.tracks.iter().copied().collect(),
                }),
        }
    }

    pub fn apply_info(&mut self, info: &notwig_core::edit::SelectionInfo<TrackId, ClipId>) {
        if let Some(time) = &info.time {
            let lanes: Vec<TrackId> = time.lanes.iter().copied().collect();
            self.select_time(time.span, &lanes);
        } else if !info.objects.is_empty() {
            let clips: Vec<(ClipId, TrackId)> = info
                .objects
                .iter()
                .map(|entry| (entry.object, entry.lane))
                .collect();
            self.select_clips(&clips);
        } else {
            self.clips.clear();
            self.time = None;
        }
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
    pub const MAX_PPB: f32 = 1_000_000.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Warning,
    Error,
}

const TRANSIENT_NOTIFICATION_TTL: f64 = 6.0;
const MAX_NOTIFICATIONS: usize = 64;

/// A user-facing notification shown in the shell overlay.
///
/// Transient notifications expire after [`UiState::NOTIFICATION_TTL_SECONDS`]
/// once [`UiState::expire_notifications`] is called. Persistent notifications
/// (renders, imports, ...) stay until explicitly removed or completed.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub kind: NotificationKind,
    pub message: String,
    /// When `true`, the notification ignores TTL expiry.
    pub persistent: bool,
    /// Timestamp, in seconds on the caller's clock, when this was created.
    pub created_at: f64,
    /// Optional progress in `0.0..=1.0` for long-running operations.
    pub progress: Option<f32>,
    /// Optional stable identity for long-running operations.
    ///
    /// Tags are not rendered. They let callers remove the notification for a
    /// specific operation when several overlapping operations exist.
    pub tag: Option<String>,
}

impl Notification {
    fn transient(kind: NotificationKind, message: String, created_at: f64) -> Self {
        Self {
            kind,
            message,
            persistent: false,
            created_at,
            progress: None,
            tag: None,
        }
    }

    fn persistent(kind: NotificationKind, message: String, created_at: f64) -> Self {
        Self {
            kind,
            message,
            persistent: true,
            created_at,
            progress: None,
            tag: None,
        }
    }

    /// Returns `true` when a transient notification has outlived its TTL.
    pub fn is_expired(&self, now: f64) -> bool {
        !self.persistent && now - self.created_at >= TRANSIENT_NOTIFICATION_TTL
    }
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
    /// How long a transient notification lives, in seconds.
    pub const NOTIFICATION_TTL_SECONDS: f64 = TRANSIENT_NOTIFICATION_TTL;
    /// Maximum number of retained notifications; oldest are evicted first.
    pub const NOTIFICATION_CAP: usize = MAX_NOTIFICATIONS;

    /// Records a block peak and returns the held maximum since the last reset.
    pub fn record_peak(&mut self, track_id: TrackId, peak: f32) -> f32 {
        let held = self.peak_hold.entry(track_id).or_insert(0.0);
        *held = held.max(peak.max(0.0));
        *held
    }

    pub fn reset_peak(&mut self, track_id: TrackId) {
        self.peak_hold.remove(&track_id);
    }

    /// Pushes a transient notification that expires after the TTL.
    pub fn notify(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notify_with(kind, message, false);
    }

    /// Pushes a notification that remains until explicitly removed.
    pub fn notify_persistent(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notify_with(kind, message, true);
    }

    /// Pushes a persistent progress notification with a stable tag.
    ///
    /// Use an indeterminate progress value (`0.0`) when the operation cannot
    /// report real progress. Remove the notification with
    /// [`UiState::remove_notification_by_tag`] when the operation finishes.
    pub fn notify_persistent_progress(
        &mut self,
        kind: NotificationKind,
        message: impl Into<String>,
        progress: f32,
        tag: impl Into<String>,
    ) {
        let message = message.into();
        let mut note = Notification::persistent(kind, message, self.clock);
        note.progress = Some(progress.clamp(0.0, 1.0));
        note.tag = Some(tag.into());
        self.notifications.push(note);
        self.enforce_notification_cap();
    }

    /// Pushes a notification, choosing whether it is persistent.
    pub fn notify_with(
        &mut self,
        kind: NotificationKind,
        message: impl Into<String>,
        persistent: bool,
    ) {
        let message = message.into();
        let note = if persistent {
            Notification::persistent(kind, message, self.clock)
        } else {
            Notification::transient(kind, message, self.clock)
        };
        self.notifications.push(note);
        self.enforce_notification_cap();
    }

    /// Removes expired transient notifications.
    ///
    /// Call once per frame with the current time in seconds (any monotonic
    /// origin). The timestamp is also remembered as the creation time for
    /// later notifications, so transient entries always get a coherent age.
    pub fn expire_notifications(&mut self, now: f64) {
        let now = now.max(self.clock);
        self.clock = now;
        self.notifications.retain(|note| !note.is_expired(now));
        self.enforce_notification_cap();
    }

    /// Removes a notification by position, returning it when present.
    pub fn remove_notification(&mut self, index: usize) -> Option<Notification> {
        if index < self.notifications.len() {
            Some(self.notifications.remove(index))
        } else {
            None
        }
    }

    /// Removes the notification identified by `tag`, returning it when present.
    pub fn remove_notification_by_tag(&mut self, tag: &str) -> Option<Notification> {
        let index = self
            .notifications
            .iter()
            .position(|note| note.tag.as_deref() == Some(tag))?;
        self.remove_notification(index)
    }

    /// Sets or clears progress (`Some(0.0..=1.0)` or `None`) for a notification.
    ///
    /// Returns `false` when `index` is out of range. Setting progress does not
    /// change persistence; use [`UiState::notify_persistent`] for progress
    /// notifications that must survive the TTL.
    pub fn set_notification_progress(&mut self, index: usize, progress: Option<f32>) -> bool {
        let Some(note) = self.notifications.get_mut(index) else {
            return false;
        };
        note.progress = progress.map(|value| value.clamp(0.0, 1.0));
        true
    }

    /// Removes every notification.
    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
    }

    fn enforce_notification_cap(&mut self) {
        while self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications.remove(0);
        }
    }
}
