use notwig_core::{Curve, CurvePoint, TimeMus, shape_ratio};

fn time(beat: i64) -> TimeMus {
    TimeMus::new(beat, 1)
}

fn curve(points: impl IntoIterator<Item = CurvePoint>) -> Curve {
    Curve::from_points(points).unwrap()
}

#[test]
fn empty_curves_hold_the_fallback() {
    let curve = Curve::new();
    assert!(curve.is_empty());
    assert_eq!(curve.value_at(time(4), -3.0), -3.0);
}

#[test]
fn linear_curves_interpolate_between_points() {
    let curve = curve([CurvePoint::new(time(0), 0.0), CurvePoint::new(time(4), 8.0)]);
    assert_eq!(curve.value_at(time(0), 0.0), 0.0);
    assert_eq!(curve.value_at(time(2), 0.0), 4.0);
    assert_eq!(curve.value_at(time(4), 0.0), 8.0);
    assert_eq!(curve.value_at(time(-2), 0.0), 0.0);
    assert_eq!(curve.value_at(time(9), 0.0), 8.0);
}

#[test]
fn curvature_preserves_endpoints_and_eases_in_or_out() {
    let linear = curve([CurvePoint::new(time(0), 0.0), CurvePoint::new(time(4), 4.0)]);
    let ease_in = curve([
        CurvePoint::new(time(0), 0.0).with_curvature(1.0),
        CurvePoint::new(time(4), 4.0),
    ]);
    let ease_out = curve([
        CurvePoint::new(time(0), 0.0).with_curvature(-1.0),
        CurvePoint::new(time(4), 4.0),
    ]);
    assert_eq!(ease_in.value_at(time(0), 0.0), 0.0);
    assert_eq!(ease_in.value_at(time(4), 0.0), 4.0);
    assert!(ease_in.value_at(time(1), 0.0) < linear.value_at(time(1), 0.0));
    assert!(ease_in.value_at(time(3), 0.0) < linear.value_at(time(3), 0.0));
    assert!(ease_out.value_at(time(1), 0.0) > linear.value_at(time(1), 0.0));
    assert!(ease_out.value_at(time(3), 0.0) > linear.value_at(time(3), 0.0));
}

#[test]
fn shape_ratio_is_monotonic_across_the_supported_range() {
    for curvature in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        let mut previous = shape_ratio(0.0, curvature);
        for step in 1..=100 {
            let value = shape_ratio(f64::from(step) / 100.0, curvature);
            assert!(
                value + f64::EPSILON >= previous,
                "curvature {curvature} must not reverse"
            );
            previous = value;
        }
    }
}

#[test]
fn from_points_sorts_stably_and_allows_duplicate_times() {
    let curve = curve([
        CurvePoint::new(time(4), 4.0),
        CurvePoint::new(time(0), 0.0),
        CurvePoint::new(time(0), 1.0),
    ]);
    assert_eq!(curve.points()[0].time, time(0));
    assert_eq!(curve.points()[0].value, 0.0);
    assert_eq!(curve.points()[1].time, time(0));
    assert_eq!(curve.points()[1].value, 1.0);
    assert_eq!(curve.points()[2].time, time(4));
    assert_eq!(curve.points_at(time(0)).len(), 2);
}

#[test]
fn duplicate_times_form_a_vertical_step() {
    let curve = curve([
        CurvePoint::new(time(0), 0.0),
        CurvePoint::new(time(2), 1.0),
        CurvePoint::new(time(2), 4.0),
        CurvePoint::new(time(4), 8.0),
    ]);
    assert!((curve.value_at(time(1), 0.0) - 0.5).abs() < 1e-9);
    assert_eq!(curve.value_at(time(2), 0.0), 4.0);
    assert!((curve.value_at(time(3), 0.0) - 6.0).abs() < 1e-9);
}

#[test]
fn from_points_rejects_non_finite_values_and_curvature() {
    assert!(Curve::from_points([CurvePoint::new(time(0), f64::NAN)]).is_err());
    assert!(
        Curve::from_points([CurvePoint::new(time(0), 0.0).with_curvature(f64::INFINITY)]).is_err()
    );
}
