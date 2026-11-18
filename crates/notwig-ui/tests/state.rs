use notwig_core::ids::{Clip, Device, DeviceChain, Track};
use notwig_core::{Id, RangeMus, TimeMus};
use notwig_ui::{Selection, Viewport};
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
