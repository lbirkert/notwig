use egui::Modifiers;
use notwig_core::TimeMus;
use notwig_core::time::TimeSignature;
use notwig_ui::{
    SNAP_LEVELS, SnapLevel, SnapSettings, adaptive_step, snap_step, snap_step_label, snap_time,
};

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

fn bar(numerator: u8, denominator: u8) -> TimeMus {
    TimeSignature::new(numerator, denominator).bar_length()
}

fn is_integer_ratio(a: TimeMus, b: TimeMus) -> bool {
    if b.num() == 0 {
        return false;
    }
    let numerator = i128::from(a.num()) * i128::from(b.den());
    let denominator = i128::from(a.den()) * i128::from(b.num());
    denominator != 0 && numerator % denominator == 0
}

fn commensurate(step: TimeMus, bar_length: TimeMus) -> bool {
    is_integer_ratio(bar_length, step) || is_integer_ratio(step, bar_length)
}

#[test]
fn time_signature_lengths_are_exact() {
    let cases = [
        (4, 4, TimeMus::new(1, 1), TimeMus::new(4, 1)),
        (3, 4, TimeMus::new(1, 1), TimeMus::new(3, 1)),
        (6, 8, TimeMus::new(1, 2), TimeMus::new(3, 1)),
        (7, 8, TimeMus::new(1, 2), TimeMus::new(7, 2)),
        (12, 8, TimeMus::new(1, 2), TimeMus::new(6, 1)),
    ];
    for (numerator, denominator, beat, bar_length) in cases {
        let signature = TimeSignature::new(numerator, denominator);
        assert_eq!(
            signature.beat_length(),
            beat,
            "unexpected beat length for {numerator}/{denominator}"
        );
        assert_eq!(
            signature.bar_length(),
            bar_length,
            "unexpected bar length for {numerator}/{denominator}"
        );
    }
}

#[test]
fn adaptive_steps_are_commensurate_with_the_bar() {
    for (numerator, denominator) in [(4, 4), (3, 4), (6, 8), (7, 8), (12, 8)] {
        let bar_length = bar(numerator, denominator);
        for ppb in ZOOM_LEVELS {
            let step = adaptive_step(ppb, bar_length);
            assert!(
                commensurate(step, bar_length),
                "adaptive step {step} does not align with the {numerator}/{denominator} bar \
                 {bar_length} at {ppb} px/beat"
            );
        }
    }
}

#[test]
fn adaptive_steps_follow_the_signature_denominator() {
    // 7/8 bars cannot be divided by a quarter note, but they can by an eighth.
    assert_eq!(adaptive_step(100.0, bar(7, 8)), TimeMus::new(1, 2));
    // 6/8 has a three-beat bar, so a whole beat is a valid adaptive step.
    assert_eq!(adaptive_step(64.0, bar(6, 8)), TimeMus::new(1, 1));
    // 4/4 keeps the existing whole-beat choice at the default zoom.
    assert_eq!(adaptive_step(64.0, bar(4, 4)), TimeMus::new(1, 1));
}

#[test]
fn snap_step_is_monotonic_as_zoom_increases() {
    for (numerator, denominator) in [(4, 4), (3, 4), (6, 8), (7, 8), (12, 8)] {
        let bar_length = bar(numerator, denominator);
        let mut previous = None;
        for ppb in ZOOM_LEVELS {
            let step = snap_step(ppb, bar_length);
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
fn snap_step_keeps_chosen_grid_readable_when_zoomed_in() {
    for ppb in ZOOM_LEVELS.into_iter().filter(|ppb| *ppb >= 40.0) {
        let step = snap_step(ppb, bar(4, 4));
        let pixels = step.as_f64() as f32 * ppb;
        assert!(
            pixels >= 40.0 - 1e-3,
            "step {step} is only {pixels} px wide at {ppb} px/beat"
        );
    }
}

#[test]
fn manual_levels_cover_the_supported_range_including_eighth_notes() {
    let labels: Vec<String> = SNAP_LEVELS.iter().map(|level| level.label()).collect();
    assert!(labels.contains(&"1/8".to_string()));
    assert_eq!(labels.first().map(String::as_str), Some("2/1"));
    assert_eq!(labels.last().map(String::as_str), Some("1/65536"));
    for expected in [
        "2/1", "1/1", "1/2", "1/4", "1/8", "1/16", "1/32", "1/64", "1/128", "1/256", "1/512",
        "1/1024", "1/2048", "1/4096", "1/8192", "1/16384", "1/32768", "1/65536",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing snap level {expected}"
        );
    }
}

#[test]
fn manual_level_snaps_to_the_selected_note_value() {
    let snap = SnapSettings {
        enabled: true,
        adaptive: false,
        level: SnapLevel::new(1, 8),
    };
    let five_sixteenths = TimeMus::new(5, 16);
    assert_eq!(
        snap.snap(five_sixteenths, 64.0, bar(4, 4), false),
        TimeMus::new(1, 2)
    );
    assert_eq!(snap.step(64.0, bar(4, 4)), TimeMus::new(1, 2));
    assert_eq!(
        snap_step_label(snap.step(64.0, bar(4, 4)), bar(4, 4)),
        "1/8"
    );
}

#[test]
fn disabling_snap_and_modifier_bypass_return_raw_times() {
    let snap = SnapSettings::default();
    let time = TimeMus::new(37, 16);
    assert_eq!(snap.snap(time, 64.0, bar(4, 4), false), TimeMus::new(2, 1));
    assert_eq!(snap.snap(time, 64.0, bar(4, 4), true), time);
    assert!(snap.bypassed(Modifiers {
        shift: true,
        ..Modifiers::NONE
    }));
    assert!(snap.bypassed(Modifiers {
        alt: true,
        ..Modifiers::NONE
    }));

    let disabled = SnapSettings {
        enabled: false,
        ..snap
    };
    assert_eq!(disabled.snap(time, 64.0, bar(4, 4), false), time);
    assert!(disabled.bypassed(Modifiers::NONE));
}

#[test]
fn a_fine_manual_level_is_the_one_effective_step() {
    let snap = SnapSettings {
        enabled: true,
        adaptive: false,
        level: SnapLevel::new(1, 65536),
    };
    let step = snap.step(64.0, bar(4, 4));
    assert_eq!(step, SnapLevel::new(1, 65536).step());
    assert_eq!(
        snap_step_label(step, bar(4, 4)),
        "1/65536",
        "the label must describe the same step used for snapping and painting"
    );
}

#[test]
fn snap_step_label_reports_real_bar_multiples() {
    let seven_eight = bar(7, 8);
    assert_eq!(snap_step_label(seven_eight, seven_eight), "1 bar");
    assert_eq!(
        snap_step_label(seven_eight + seven_eight, seven_eight),
        "2 bars"
    );
    assert_eq!(
        snap_step_label(TimeMus::new(1, 1), TimeMus::new(4, 1)),
        "1/4"
    );
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
        let step = snap_step(ppb, bar(4, 4));
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
