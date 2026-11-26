use notwig_core::ids::{Clip, Device, DeviceChain, Track};
use notwig_core::{Id, RangeMus, TimeMus};
use notwig_ui::{NotificationKind, Selection, UiState, Viewport};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

#[test]
fn viewport_roundtrips_beats() {
    let viewport = Viewport::default();
    let beat = TimeMus::new(4, 1);
    let x = viewport.beat_to_x(beat, 230.0);
    assert!((viewport.x_to_beat(x, 230.0) - beat).as_f64().abs() < 1e-6);
}

#[test]
fn selection_is_additive_per_kind_and_keeps_primary() {
    let mut selection = Selection::default();
    let track = id::<Track>(1);
    selection.select_track(track, false);
    assert!(selection.tracks.contains(&track));
    assert_eq!(selection.primary_track(), Some(track));

    let clip = id::<Clip>(2);
    selection.select_clip(clip, track, false);
    assert!(selection.clips.contains(&clip));
    assert_eq!(selection.primary_clip(), Some(clip));
    assert_eq!(selection.primary_track(), Some(track));

    let device_a = id::<Device>(3);
    let device_b = id::<Device>(4);
    selection.select_device(device_a, track, false);
    selection.select_device(device_b, track, true);
    assert_eq!(selection.devices.len(), 2);
    assert!(selection.devices.contains(&device_a));
    assert!(selection.devices.contains(&device_b));
    assert_eq!(selection.primary_device(), Some(device_b));

    selection.select_device(device_b, track, true);
    assert_eq!(selection.primary_device(), Some(device_a));
    let _ = id::<DeviceChain>(5);
}

#[test]
fn time_selection_clears_other_modes_and_tracks_lanes() {
    let mut selection = Selection::default();
    let a = id::<Track>(20);
    let b = id::<Track>(21);
    selection.select_track(a, false);
    selection.select_time(
        RangeMus::new(TimeMus::new(1, 1), TimeMus::new(4, 1)).unwrap(),
        &[a, b],
    );
    let time = selection.time.as_ref().unwrap();
    assert_eq!(time.range.end, TimeMus::new(4, 1));
    assert!(time.contains_track(a));
    assert!(time.contains_track(b));
    assert!(selection.clips.is_empty());
    assert_eq!(selection.tracks.len(), 2);
    selection.clear();
    assert!(selection.time.is_none());
}

#[test]
fn bar_selection_and_additive_node_selection_share_one_model() {
    let mut selection = Selection::default();
    let track = id::<Track>(30);
    let clip = id::<Clip>(31);
    let range = RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap();
    selection.select_time(range, &[track]);
    assert!(selection.time.is_some());
    assert!(selection.clips.is_empty());

    selection.select_clip(clip, track, false);
    assert!(selection.time.is_none());
    assert!(selection.clips.contains(&clip));

    selection.select_time(range, &[track]);
    selection.select_clip(clip, track, true);
    assert!(selection.time.is_some());
    assert!(selection.clips.contains(&clip));
}

#[test]
fn selection_retain_prunes_primary() {
    let mut selection = Selection::default();
    let a = id::<Track>(10);
    let b = id::<Track>(11);
    selection.select_track(a, false);
    selection.select_track(b, true);
    selection.tracks.retain(|track| *track == a);
    assert_eq!(selection.tracks.len(), 1);
    assert_eq!(selection.primary_track(), Some(a));
}

#[test]
fn track_selection_supports_add_and_range() {
    let a = id::<Track>(30);
    let b = id::<Track>(31);
    let c = id::<Track>(32);
    let d = id::<Track>(33);
    let order = [a, b, c, d];

    let mut selection = Selection::default();
    selection.select_track(b, false);
    selection.add_track(d);
    assert_eq!(selection.tracks.len(), 2);
    assert!(selection.tracks.contains(&b));
    assert!(selection.tracks.contains(&d));

    selection.select_track_range(b, d, &order);
    assert_eq!(selection.tracks.len(), 3);
    assert!(selection.tracks.contains(&b));
    assert!(selection.tracks.contains(&c));
    assert!(selection.tracks.contains(&d));
    assert_eq!(selection.primary_track(), Some(d));

    selection.select_track_range(d, a, &order);
    assert_eq!(selection.tracks.len(), 4);
    assert_eq!(selection.primary_track(), Some(a));
}

#[test]
fn transient_notifications_expire_after_ttl() {
    let mut state = UiState::default();
    state.notify(NotificationKind::Info, "saved");
    assert_eq!(state.notifications.len(), 1);
    assert!(!state.notifications[0].persistent);
    assert_eq!(state.notifications[0].progress, None);

    state.expire_notifications(UiState::NOTIFICATION_TTL_SECONDS / 2.0);
    assert_eq!(state.notifications.len(), 1);

    state.expire_notifications(UiState::NOTIFICATION_TTL_SECONDS);
    assert_eq!(state.notifications.len(), 0);
}

#[test]
fn persistent_notifications_survive_ttl_until_removed() {
    let mut state = UiState::default();
    state.notify_persistent(NotificationKind::Warning, "rendering");

    state.expire_notifications(UiState::NOTIFICATION_TTL_SECONDS * 100.0);
    assert_eq!(state.notifications.len(), 1);
    assert!(state.notifications[0].persistent);
    assert!(!state.notifications[0].is_expired(1_000.0));

    assert!(state.set_notification_progress(0, Some(0.42)));
    assert_eq!(state.notifications[0].progress, Some(0.42));
    assert!(state.set_notification_progress(0, Some(4.0)));
    assert_eq!(state.notifications[0].progress, Some(1.0));
    assert!(!state.set_notification_progress(9, Some(0.5)));

    assert!(state.remove_notification(0).is_some());
    assert_eq!(state.notifications.len(), 0);
}

#[test]
fn notifications_created_after_clock_advances_use_current_time() {
    let mut state = UiState::default();
    state.expire_notifications(100.0);
    state.notify(NotificationKind::Info, "late");
    assert_eq!(state.notifications[0].created_at, 100.0);

    state.expire_notifications(100.0 + UiState::NOTIFICATION_TTL_SECONDS - 0.5);
    assert_eq!(state.notifications.len(), 1);

    state.expire_notifications(100.0 + UiState::NOTIFICATION_TTL_SECONDS);
    assert_eq!(state.notifications.len(), 0);
}

#[test]
fn notification_cap_evicts_oldest_first() {
    let mut state = UiState::default();
    for index in 0..UiState::NOTIFICATION_CAP + 6 {
        state.notify(NotificationKind::Info, format!("note {index}"));
    }

    assert_eq!(state.notifications.len(), UiState::NOTIFICATION_CAP);
    assert_eq!(state.notifications.first().unwrap().message, "note 6");
    assert_eq!(
        state.notifications.last().unwrap().message,
        format!("note {}", UiState::NOTIFICATION_CAP + 5)
    );
}

#[test]
fn unbounded_viewport_allows_negative_time() {
    let mut viewport = Viewport {
        start: TimeMus::zero(),
        pixels_per_beat: 64.0,
    };
    viewport.pan_beats_unbounded(-4.0);
    assert_eq!(viewport.start, TimeMus::new(-4, 1));
    viewport.zoom_at_unbounded(100.0, 100.0, 2.0);
    assert!(viewport.start < TimeMus::zero());
    let beat = viewport.x_to_beat_unbounded(100.0, 100.0);
    assert_eq!(beat, viewport.start);
}
