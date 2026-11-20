use notwig_core::TimeMus;
use notwig_ui::{snap_step, snap_time};

const ZOOM_LEVELS: [f32; 12] = [
    1.0,
    4.0,
    8.0,
    12.0,
    16.0,
    24.0,
    32.0,
    64.0,
    128.0,
    512.0,
    10_000.0,
    1_000_000.0,
];

#[test]
fn snap_step_never_goes_below_an_eighth_note() {
    for ppb in ZOOM_LEVELS {
        for beats_per_bar in [1u8, 2, 3, 4, 6, 7, 12] {
            let step = snap_step(ppb, beats_per_bar);
            assert!(
                step >= TimeMus::new(1, 2),
                "step {step} at {ppb} px/beat in {beats_per_bar}/4 is finer than an eighth note"
            );
        }
    }
}

#[test]
fn snap_step_is_monotonic_as_zoom_increases() {
    for beats_per_bar in [1u8, 3, 4, 6, 7, 12] {
        let mut previous = None;
        for ppb in ZOOM_LEVELS {
            let step = snap_step(ppb, beats_per_bar);
            if let Some(previous) = previous {
                assert!(
                    step <= previous,
                    "step grew from {previous} to {step} when zooming in at {ppb} px/beat"
                );
            }
            previous = Some(step);
        }
    }
}

#[test]
fn snap_step_keeps_chosen_grid_readable() {
    for ppb in ZOOM_LEVELS {
        let step = snap_step(ppb, 4);
        let pixels = step.as_f64() as f32 * ppb;
        assert!(
            pixels >= 12.0 - 1e-3 || step == TimeMus::new(1, 2),
            "step {step} is only {pixels} px wide at {ppb} px/beat"
        );
    }
}

#[test]
fn snap_time_is_idempotent_and_respects_bypass() {
    let times = [
        TimeMus::new(3, 16),
        TimeMus::new(9, 32),
        TimeMus::new(5, 8),
        TimeMus::new(7, 3),
    ];
    for ppb in ZOOM_LEVELS {
        let step = snap_step(ppb, 4);
        for time in times {
            let snapped = snap_time(time, step, false);
            assert_eq!(snap_time(snapped, step, false), snapped);
            assert_eq!(snap_time(time, step, true), time);
            let distance = if snapped >= time {
                snapped - time
            } else {
                time - snapped
            };
            assert!(
                distance.as_f64() <= step.as_f64() / 2.0 + 1e-6,
                "snapping {time} to {snapped} moved more than half a step at {ppb} px/beat"
            );
        }
    }
}
