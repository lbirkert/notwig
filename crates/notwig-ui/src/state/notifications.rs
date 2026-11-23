use super::UiState;

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
    /// When `true`, render a horizontal indeterminate loader instead of a
    /// determinate progress bar.
    pub indeterminate: bool,
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
            indeterminate: false,
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
            indeterminate: false,
            tag: None,
        }
    }

    /// Returns `true` when a transient notification has outlived its TTL.
    pub fn is_expired(&self, now: f64) -> bool {
        !self.persistent && now - self.created_at >= TRANSIENT_NOTIFICATION_TTL
    }
}

impl UiState {
    /// How long a transient notification lives, in seconds.
    pub const NOTIFICATION_TTL_SECONDS: f64 = TRANSIENT_NOTIFICATION_TTL;
    /// Maximum number of retained notifications; oldest are evicted first.
    pub const NOTIFICATION_CAP: usize = MAX_NOTIFICATIONS;

    /// Pushes a transient notification that expires after the TTL.
    pub fn notify(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notify_with(kind, message, false);
    }

    /// Pushes a notification that remains until explicitly removed.
    pub fn notify_persistent(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notify_with(kind, message, true);
    }

    /// Pushes a persistent notification with a horizontal indeterminate loader.
    ///
    /// Remove the notification with [`UiState::remove_notification_by_tag`]
    /// when the operation finishes.
    pub fn notify_persistent_indeterminate(
        &mut self,
        kind: NotificationKind,
        message: impl Into<String>,
        tag: impl Into<String>,
    ) {
        let message = message.into();
        let mut note = Notification::persistent(kind, message, self.clock);
        note.indeterminate = true;
        note.tag = Some(tag.into());
        self.notifications.push(note);
        self.enforce_notification_cap();
    }

    /// Pushes a persistent progress notification with a stable tag.
    ///
    /// Use [`UiState::notify_persistent_indeterminate`] when the operation
    /// cannot report real progress. Remove the notification with
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
