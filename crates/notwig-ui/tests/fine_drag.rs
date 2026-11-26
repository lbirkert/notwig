use notwig_ui::test_api::fine_drag::FineDrag;

#[test]
fn shift_preserves_the_coarse_offset_and_adds_fine_motion() {
    let mut drag = FineDrag::default();
    assert_eq!(drag.update(10.0, false, 0.1).offset, 10.0);
    let start = drag.update(20.0, true, 0.1);
    assert!(start.shift_started);
    assert!(!start.fine_from_start);
    assert_eq!(start.offset, 10.0);
    assert_eq!(drag.update(30.0, true, 0.1).offset, 11.0);
    assert_eq!(drag.update(30.0, false, 0.1).offset, 11.0);
    assert_eq!(drag.update(40.0, false, 0.1).offset, 21.0);
}

#[test]
fn shift_held_from_the_start_uses_fine_motion_from_the_origin() {
    let mut drag = FineDrag::default();
    let update = drag.update(40.0, true, 0.1);
    assert!(!update.shift_started);
    assert!(update.fine_from_start);
    assert_eq!(update.offset, 4.0);
    assert_eq!(drag.update(50.0, true, 0.1).offset, 5.0);
}

#[test]
fn releasing_shift_keeps_coarse_and_fine_offsets() {
    let mut drag = FineDrag::default();
    drag.update(10.0, false, 0.1);
    drag.update(20.0, true, 0.1);
    assert_eq!(drag.update(30.0, true, 0.1).offset, 11.0);
    assert_eq!(drag.update(40.0, false, 0.1).offset, 11.0);
    assert_eq!(drag.update(50.0, false, 0.1).offset, 21.0);
}

#[test]
fn set_base_replaces_the_frozen_coarse_offset() {
    let mut drag = FineDrag::default();
    drag.update(5.0, false, 0.1);
    let update = drag.update(20.0, true, 0.1);
    assert!(update.shift_started);
    assert_eq!(drag.set_base(8.0), 8.0);
    assert_eq!(drag.update(30.0, true, 0.1).offset, 9.0);
}
