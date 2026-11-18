use notwig_core::{RangeMus, TimeMus};
use notwig_ui::{clamp_node_shift, paste_target};

fn range(start: i64, end: i64) -> RangeMus {
    RangeMus::new(TimeMus::new(start, 1), TimeMus::new(end, 1)).unwrap()
}

#[test]
fn paste_target_uses_bar_then_cursor() {
    assert_eq!(
        paste_target(Some(range(2, 4)), TimeMus::new(9, 1)),
        TimeMus::new(2, 1)
    );
    assert_eq!(paste_target(None, TimeMus::new(9, 1)), TimeMus::new(9, 1));
}

#[test]
fn node_shift_clamps_the_whole_group() {
    let starts = [TimeMus::new(1, 1), TimeMus::new(3, 1)];
    assert_eq!(
        clamp_node_shift(TimeMus::new(-5, 1), &starts, None),
        TimeMus::new(-1, 1)
    );
    assert_eq!(
        clamp_node_shift(TimeMus::new(5, 1), &starts, Some(TimeMus::new(6, 1))),
        TimeMus::new(3, 1)
    );
}
