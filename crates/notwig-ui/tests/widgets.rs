use notwig_core::TimeMus;
use notwig_ui::test_api::widgets::draggable_number::{dragged_value, parse_value};
use notwig_ui::{snap_step, snap_time};

#[test]
fn snapping_step_adapts_to_zoom() {
    let four_four = TimeMus::new(4, 1);
    let far = snap_step(16.0, four_four);
    let near = snap_step(100.0, four_four);
    assert!(far > near, "zoomed out should snap coarser");
    assert_eq!(near, TimeMus::new(1, 2));
    assert!(
        snap_step(1_000.0, four_four) < near,
        "zooming further in should expose finer subdivisions"
    );
}

#[test]
fn snapping_uses_the_step() {
    let step = TimeMus::new(1, 8);
    assert_eq!(
        snap_time(TimeMus::new(3, 16), step, false),
        TimeMus::new(1, 4)
    );
    let raw = TimeMus::new(3, 16);
    assert_eq!(snap_time(raw, step, true), raw);
}

#[test]
fn drag_changes_value_and_clamps() {
    assert_eq!(dragged_value(10.0, 10.0, 0.0, 0.5, false, 0.0, 20.0), 15.0);
    assert_eq!(dragged_value(5.0, 1000.0, 0.0, 1.0, false, 0.0, 12.0), 12.0);
}

#[test]
fn fine_drag_is_slower() {
    let coarse = dragged_value(10.0, 10.0, 0.0, 0.5, false, 0.0, 20.0);
    let fine = dragged_value(10.0, 10.0, 0.0, 0.5, true, 0.0, 20.0);
    assert!((fine - 10.5).abs() < 1e-12);
    assert!(fine < coarse);
}

#[test]
fn vertical_up_increases_value() {
    assert_eq!(dragged_value(0.0, 0.0, -8.0, 0.5, false, -10.0, 10.0), 4.0);
}

#[test]
fn parse_clamps_and_falls_back() {
    assert_eq!(parse_value(" 3.5 ", 0.0, 10.0, 1.0), 3.5);
    assert_eq!(parse_value("100", 0.0, 10.0, 1.0), 10.0);
    assert_eq!(parse_value("-100", 0.0, 10.0, 1.0), 0.0);
    assert_eq!(parse_value("nope", 0.0, 10.0, 1.0), 1.0);
}
