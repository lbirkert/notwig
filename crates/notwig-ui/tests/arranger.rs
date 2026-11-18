use notwig_core::TimeMus;
use notwig_ui::NotwigUi;

#[test]
fn zoom_keeps_pointer_beat_stable() {
    let mut ui = NotwigUi::default();
    ui.timeline_left = 230.0;
    ui.state.viewport.pixels_per_beat = 64.0;
    ui.state.viewport.start = TimeMus::zero();
    ui.state.viewport.zoom_at(430.0, ui.timeline_left, 2.0);
    assert!((ui.state.viewport.pixels_per_beat - 128.0).abs() < 1e-3);
    let beat = ui.state.viewport.start.as_f64()
        + f64::from((430.0 - ui.timeline_left) / ui.state.viewport.pixels_per_beat);
    assert!((beat - 3.125).abs() < 1e-6);
}

#[test]
fn panning_clamps_at_zero() {
    let mut ui = NotwigUi::default();
    ui.state.viewport.start = TimeMus::new(10, 1);
    ui.state.viewport.pan_beats(-4.0);
    assert_eq!(ui.state.viewport.start, TimeMus::new(6, 1));
    ui.state.viewport.pan_beats(-100.0);
    assert_eq!(ui.state.viewport.start, TimeMus::zero());
}
