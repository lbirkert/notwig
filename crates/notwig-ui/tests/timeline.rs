use egui::{Pos2, Rect};
use notwig_core::edit::Edge;
use notwig_core::ids::{Clip, DeviceChain, Track};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData};
use notwig_core::{ClipEntity, Id, Meta, ProjectDoc, RangeMus, SampleRate, TimeMus};
use notwig_ui::test_api::timeline::TimelineGrid;
use notwig_ui::test_api::timeline::clip::clip_visual_rects;
use notwig_ui::test_api::timeline::{
    navigation_update, pointer_delta, resize_loop_edge, selection_range, snapped_shift,
    snapped_start, trim_range,
};
use notwig_ui::test_api::waveform::waveform_region;
use notwig_ui::{SnapLevel, SnapSettings, Viewport, WaveformCache};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn empty_doc() -> ProjectDoc {
    ProjectDoc::new_template(
        Meta {
            name: "Timeline".into(),
            created_utc: 0,
            modified_utc: 0,
            sample_rate: SampleRate::DEFAULT,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
        },
        id::<Track>(1),
        id::<DeviceChain>(100),
    )
    .unwrap()
}

fn midi_clip(n: u128, track: u128, start: TimeMus, end: TimeMus) -> ClipEntity {
    ClipEntity {
        id: id::<Clip>(n),
        track_id: id::<Track>(track),
        kind: ClipKind::Midi,
        range: RangeMus::new(start, end).unwrap(),
        name: None,
        color: None,
        muted: false,
        payload: ClipPayload::Midi(MidiClipData {
            length: end - start,
            content_start: TimeMus::zero(),
            notes: Vec::new(),
        }),
    }
}

fn render_clip_visual(
    clip: &ClipEntity,
    lane_rect: Rect,
    grid: TimelineGrid,
) -> Option<(Rect, Rect)> {
    let doc = empty_doc();
    let waveforms = WaveformCache::default();
    let ctx = egui::Context::default();
    let mut visual = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(lane_rect),
            ..Default::default()
        },
        |ctx| {
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), lane_rect);
            visual = clip_visual_rects(
                &painter, &doc, clip, clip.range, lane_rect, grid, &waveforms, false, false,
            );
        },
    );
    visual
}

#[test]
fn coordinates_round_trip_and_snap() {
    let snap = SnapSettings {
        enabled: true,
        adaptive: false,
        level: SnapLevel::new(1, 8),
    };
    let grid = TimelineGrid::new(
        TimeMus::zero(),
        64.0,
        TimeMus::new(4, 1),
        TimeMus::new(1, 1),
        snap,
    );
    let x = grid.x_for_beat(TimeMus::new(2, 1), 10.0);
    assert!((x - 138.0).abs() < 0.01);
    assert_eq!(grid.beat_for_x(x, 10.0), TimeMus::new(2, 1));
    assert_eq!(grid.snap(TimeMus::new(9, 32), false), TimeMus::new(1, 2));
}

#[test]
fn over_trimmed_waveform_only_draws_available_source() {
    let full = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 10.0));
    let (rect, start, end) = waveform_region(full, full, 100, 0, 200).unwrap();
    assert!((rect.left() - 0.0).abs() < 0.01);
    assert!((rect.right() - 50.0).abs() < 0.01);
    assert_eq!(start, 0.0);
    assert_eq!(end, 100.0);
}

#[test]
fn clipped_waveform_uses_the_visible_source_window() {
    let full = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 10.0));
    let clip = Rect::from_min_max(egui::pos2(50.0, 0.0), egui::pos2(100.0, 10.0));
    let (rect, start, end) = waveform_region(full, clip, 200, 0, 200).unwrap();
    assert!((rect.left() - 50.0).abs() < 0.01);
    assert!((rect.right() - 100.0).abs() < 0.01);
    assert_eq!(start, 100.0);
    assert_eq!(end, 200.0);
}

#[test]
fn loop_edges_resize_without_inverting_the_range() {
    let range = RangeMus::new(TimeMus::new(4, 1), TimeMus::new(8, 1)).unwrap();
    assert_eq!(
        resize_loop_edge(true, range, TimeMus::new(2, 1)),
        RangeMus::new(TimeMus::new(2, 1), TimeMus::new(8, 1)).ok()
    );
    assert_eq!(
        resize_loop_edge(false, range, TimeMus::new(12, 1)),
        RangeMus::new(TimeMus::new(4, 1), TimeMus::new(12, 1)).ok()
    );
    assert_eq!(resize_loop_edge(true, range, TimeMus::new(9, 1)), None);
    assert_eq!(resize_loop_edge(false, range, TimeMus::new(3, 1)), None);
}

#[test]
fn pointer_delta_tracks_the_press_origin() {
    let start = Pos2::new(10.0, 0.0);
    assert!((pointer_delta(start, Pos2::new(90.0, 0.0), 80.0) - 1.0).abs() < 1e-9);
}

#[test]
fn snapped_start_and_shift_use_the_absolute_anchor() {
    let anchor = TimeMus::new(1, 1);
    let start = snapped_start(anchor, 0.13, TimeMus::new(1, 4), false);
    assert_eq!(start, TimeMus::new(5, 4));
    let shift = snapped_shift(anchor, 0.13, TimeMus::new(1, 4), false);
    assert_eq!(shift, TimeMus::new(1, 4));
}

#[test]
fn the_grid_paints_and_snaps_with_the_same_step() {
    let snap = SnapSettings {
        enabled: true,
        adaptive: false,
        level: SnapLevel::new(1, 8),
    };
    let bar_length = TimeMus::new(4, 1);
    let grid = TimelineGrid::new(TimeMus::zero(), 64.0, bar_length, TimeMus::new(1, 1), snap);
    assert_eq!(grid.step, snap.step(64.0, bar_length));
    assert_eq!(grid.snap(TimeMus::new(9, 32), false), TimeMus::new(1, 2));
}

#[test]
fn selection_range_is_ordered_and_rejects_empty() {
    assert_eq!(
        selection_range(TimeMus::new(2, 1), TimeMus::new(1, 1)),
        RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).ok()
    );
    assert_eq!(
        selection_range(TimeMus::new(1, 1), TimeMus::new(1, 1)),
        None
    );
}

#[test]
fn trim_range_snaps_and_respects_the_minimum_length() {
    let origin = RangeMus::new(TimeMus::new(2, 1), TimeMus::new(6, 1)).unwrap();
    let grid = TimeMus::new(1, 4);
    let min_len = TimeMus::new(1, 16);

    assert_eq!(
        trim_range(origin, Edge::Start, 1.0, grid, false, min_len),
        RangeMus::new(TimeMus::new(3, 1), TimeMus::new(6, 1)).unwrap()
    );
    assert_eq!(
        trim_range(origin, Edge::End, 1.0, grid, false, min_len),
        RangeMus::new(TimeMus::new(2, 1), TimeMus::new(7, 1)).unwrap()
    );

    let snapped = trim_range(origin, Edge::Start, 0.13, grid, false, min_len);
    assert_eq!(snapped.start, TimeMus::new(9, 4));
    let bypassed = trim_range(origin, Edge::Start, 0.13, grid, true, min_len);
    assert_eq!(bypassed.start, TimeMus::new(2_130_000, 1_000_000));

    let clamped_start = trim_range(origin, Edge::Start, 100.0, grid, false, min_len);
    assert_eq!(clamped_start.start, origin.end - min_len);
    let clamped_end = trim_range(origin, Edge::End, -100.0, grid, false, min_len);
    assert_eq!(clamped_end.end, origin.start + min_len);
    assert_eq!(clamped_start.length(), min_len);
    assert_eq!(clamped_end.length(), min_len);
}

#[test]
fn navigation_pans_without_crossing_zero_and_clamps_zoom() {
    let mut viewport = Viewport::default();
    navigation_update(
        &mut viewport,
        100.0,
        0.0,
        1.0,
        false,
        egui::Vec2::ZERO,
        egui::Vec2::new(64.0, 0.0),
    );
    assert_eq!(viewport.start, TimeMus::zero());

    navigation_update(
        &mut viewport,
        100.0,
        0.0,
        1.0,
        false,
        egui::Vec2::ZERO,
        egui::Vec2::new(-64.0, 0.0),
    );
    assert_eq!(viewport.start, TimeMus::new(1, 1));

    viewport.pixels_per_beat = Viewport::MAX_PPB;
    navigation_update(
        &mut viewport,
        0.0,
        0.0,
        2.0,
        false,
        egui::Vec2::ZERO,
        egui::Vec2::ZERO,
    );
    assert_eq!(viewport.pixels_per_beat, Viewport::MAX_PPB);
}

#[test]
fn navigation_zoom_keeps_the_beat_under_the_pointer() {
    let mut viewport = Viewport::default();
    let left = 40.0;
    let pointer = 360.0;
    let before = viewport.start.as_f64() + f64::from((pointer - left) / viewport.pixels_per_beat);
    navigation_update(
        &mut viewport,
        pointer,
        left,
        2.0,
        false,
        egui::Vec2::ZERO,
        egui::Vec2::ZERO,
    );
    let after = viewport.start.as_f64() + f64::from((pointer - left) / viewport.pixels_per_beat);
    assert!((before - after).abs() < 1e-6);
}

#[test]
fn sub_pixel_clip_still_gets_a_visible_hit_rect() {
    let lane_rect = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 100.0));
    let grid = TimelineGrid::new(
        TimeMus::zero(),
        1.0,
        TimeMus::new(4, 1),
        TimeMus::new(1, 1),
        SnapSettings::default(),
    );
    let clip = midi_clip(10, 2, TimeMus::zero(), TimeMus::new(1, 1_000));
    let (clip_rect, name_rect) =
        render_clip_visual(&clip, lane_rect, grid).expect("narrow clips must not be culled");
    assert!(
        clip_rect.width() >= 2.0,
        "clip body must keep a visible width, got {}",
        clip_rect.width()
    );
    assert!(
        name_rect.width() >= 2.0,
        "the clip hit target must stay usable, got {}",
        name_rect.width()
    );
    assert!(clip_rect.intersects(lane_rect));
    assert!(name_rect.intersects(lane_rect));
}

#[test]
fn normal_width_clip_keeps_its_musical_width() {
    let lane_rect = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 100.0));
    let grid = TimelineGrid::new(
        TimeMus::zero(),
        100.0,
        TimeMus::new(4, 1),
        TimeMus::new(1, 1),
        SnapSettings::default(),
    );
    let clip = midi_clip(11, 2, TimeMus::zero(), TimeMus::new(2, 1));
    let (clip_rect, _) = render_clip_visual(&clip, lane_rect, grid).unwrap();
    assert!((clip_rect.width() - 200.0).abs() < 0.01);
}

#[test]
fn clips_outside_the_lane_are_culled() {
    let lane_rect = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 100.0));
    let grid = TimelineGrid::new(
        TimeMus::zero(),
        100.0,
        TimeMus::new(4, 1),
        TimeMus::new(1, 1),
        SnapSettings::default(),
    );
    let left = midi_clip(12, 2, TimeMus::new(-10, 1), TimeMus::new(-19, 2));
    assert!(render_clip_visual(&left, lane_rect, grid).is_none());
    let right = midi_clip(13, 2, TimeMus::new(10, 1), TimeMus::new(11, 1));
    assert!(render_clip_visual(&right, lane_rect, grid).is_none());
}
