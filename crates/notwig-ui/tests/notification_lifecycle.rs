use notwig_ui::{NotificationKind, UiState};

#[test]
fn persistent_progress_notification_survives_ttl_and_removes_by_tag() {
    let mut state = UiState::default();
    state.notify_persistent_progress(
        NotificationKind::Info,
        "Analyzing audio…",
        0.0,
        "audio-import:1",
    );

    state.expire_notifications(UiState::NOTIFICATION_TTL_SECONDS * 100.0);
    assert_eq!(state.notifications.len(), 1);
    assert!(state.notifications[0].persistent);
    assert_eq!(state.notifications[0].progress, Some(0.0));
    assert_eq!(
        state.notifications[0].tag.as_deref(),
        Some("audio-import:1")
    );

    let removed = state
        .remove_notification_by_tag("audio-import:1")
        .expect("tagged notification is removed");
    assert_eq!(removed.message, "Analyzing audio…");
    assert_eq!(state.notifications, Vec::new());
}

#[test]
fn removing_by_tag_leaves_overlapping_operations_untouched() {
    let mut state = UiState::default();
    state.notify_persistent_progress(
        NotificationKind::Info,
        "Analyzing audio…",
        0.0,
        "audio-import:1",
    );
    state.notify_persistent_progress(
        NotificationKind::Info,
        "Analyzing audio…",
        0.0,
        "audio-import:2",
    );
    state.notify_persistent_progress(
        NotificationKind::Info,
        "Encoding audio…",
        0.0,
        "render-encoding",
    );

    assert!(state.remove_notification_by_tag("audio-import:1").is_some());
    assert_eq!(state.notifications.len(), 2);
    assert_eq!(
        state.notifications[0].tag.as_deref(),
        Some("audio-import:2"),
        "removal must not disturb the other import"
    );
    assert_eq!(
        state.notifications[1].tag.as_deref(),
        Some("render-encoding")
    );

    assert!(
        state.remove_notification_by_tag("audio-import:1").is_none(),
        "removing an absent tag is a no-op"
    );
}

#[test]
fn tagged_progress_clamps_progress() {
    let mut state = UiState::default();
    state.notify_persistent_progress(NotificationKind::Info, "Encoding audio…", 4.0, "encode");

    assert_eq!(state.notifications[0].progress, Some(1.0));
}
