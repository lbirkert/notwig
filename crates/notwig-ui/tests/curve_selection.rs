use std::collections::BTreeSet;

use notwig_core::{Curve, CurvePoint, RangeMus, TimeMus};
use notwig_ui::extension_ui::{CurveEditActions, apply_curve_edit_actions};

fn curve() -> Curve {
    Curve::from_points([
        CurvePoint::new(TimeMus::zero(), 0.1),
        CurvePoint::new(TimeMus::new(1, 1), 0.2),
        CurvePoint::new(TimeMus::new(2, 1), 0.3),
    ])
    .expect("curve")
}

fn actions() -> CurveEditActions {
    CurveEditActions::default()
}

#[test]
fn copy_cut_and_delete_match_clip_gain_semantics() {
    let mut curve = curve();
    let mut selected = BTreeSet::from([1]);
    let mut time_selection = None;

    let copy = apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            copy: true,
            ..actions()
        },
        &[],
    );
    assert!(!copy.changed);
    assert_eq!(copy.copied, Some(vec![curve.points()[1]]));

    let cut = apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            cut: true,
            ..actions()
        },
        &[],
    );
    assert!(cut.changed);
    assert_eq!(curve.points().len(), 2);
    assert!(selected.is_empty());
    assert_eq!(cut.copied.unwrap().len(), 1);

    let mut selected = BTreeSet::from([0]);
    let delete = apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            delete: true,
            ..actions()
        },
        &[],
    );
    assert!(delete.changed);
    assert_eq!(curve.points().len(), 1);
}

#[test]
fn select_all_duplicate_and_paste_use_the_shared_selection() {
    let mut curve = curve();
    let mut selected = BTreeSet::new();
    let mut time_selection =
        Some(RangeMus::new(TimeMus::zero(), TimeMus::new(1, 1)).expect("time selection"));

    apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            select_all: true,
            ..actions()
        },
        &[],
    );
    assert_eq!(selected.len(), 3);

    let duplicate = apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            duplicate: true,
            ..actions()
        },
        &[],
    );
    assert!(duplicate.changed);
    assert_eq!(curve.points().len(), 6);
    assert_eq!(selected.len(), 3);

    let clipboard = vec![CurvePoint::new(TimeMus::new(1, 1), 0.9)];
    let paste = apply_curve_edit_actions(
        &mut curve,
        &mut selected,
        &mut time_selection,
        &CurveEditActions {
            paste: true,
            paste_at: TimeMus::new(8, 1),
            ..actions()
        },
        &clipboard,
    );
    assert!(paste.changed);
    assert!(
        curve
            .points()
            .iter()
            .any(|point| point.time == TimeMus::new(8, 1)
                && (point.value - 0.9).abs() < f64::EPSILON)
    );
}
