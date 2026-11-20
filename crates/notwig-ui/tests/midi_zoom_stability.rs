use notwig_core::TimeMus;
use notwig_ui::Viewport;

fn anchor_beat(viewport: Viewport, pointer_x: f32, content_left: f32) -> f64 {
    viewport.start.as_f64() + f64::from((pointer_x - content_left) / viewport.pixels_per_beat)
}

#[test]
fn zoom_keeps_the_beat_under_the_pointer_while_zooming_in() {
    let mut viewport = Viewport {
        start: TimeMus::new(3, 1),
        pixels_per_beat: 80.0,
        scroll_track: 0,
    };
    let content_left = 56.0;
    let pointer = 640.0;
    let anchor = anchor_beat(viewport, pointer, content_left);

    for _ in 0..80 {
        viewport.zoom_at(pointer, content_left, 1.1);
        let drift = (anchor_beat(viewport, pointer, content_left) - anchor).abs();
        let drift_px = drift * f64::from(viewport.pixels_per_beat);
        assert!(
            drift_px < 1.0,
            "anchor drifted by {drift_px} px at {} px/beat",
            viewport.pixels_per_beat
        );
    }
}

#[test]
fn zooming_in_and_back_out_returns_to_the_same_view() {
    let mut viewport = Viewport {
        start: TimeMus::new(2, 1),
        pixels_per_beat: 80.0,
        scroll_track: 0,
    };
    let original = viewport;
    let content_left = 56.0;
    let pointer = 400.0;

    for _ in 0..50 {
        viewport.zoom_at(pointer, content_left, 1.2);
    }
    for _ in 0..50 {
        viewport.zoom_at(pointer, content_left, 1.0 / 1.2);
    }

    assert!((viewport.pixels_per_beat - original.pixels_per_beat).abs() < 1e-3);
    let drift = (viewport.start.as_f64() - original.start.as_f64()).abs();
    let drift_px = drift * f64::from(original.pixels_per_beat);
    assert!(
        drift_px < 1.0,
        "view drifted by {drift_px} px after zoom roundtrip"
    );
}

#[test]
fn zoom_never_creates_a_negative_start() {
    let mut viewport = Viewport {
        start: TimeMus::zero(),
        pixels_per_beat: 64.0,
        scroll_track: 0,
    };
    for _ in 0..40 {
        viewport.zoom_at(0.0, 100.0, 2.0);
    }
    assert_eq!(viewport.start, TimeMus::zero());
}
