use egui::Modifiers;
use notwig_core::TimeMus;
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
fn snap_step_keeps_chosen_grid_readable_when_zoomed_in() {
    for ppb in ZOOM_LEVELS.into_iter().filter(|ppb| *ppb >= 40.0) {
        let step = snap_step(ppb, 4);
        let pixels = step.as_f64() as f32 * ppb;
        assert!(
            pixels >= 40.0 - 1e-3,
            "step {step} is only {pixels} px wide at {ppb} px/beat"
        );
    }
}

#[test]
fn adaptive_mode_uses_larger_spacing_than_the_old_twelve_pixel_grid() {
    // The previous implementation picked a 1/8 note (0.5 beats) at the
    // default zoom; the new threshold intentionally picks a whole beat.
    assert_eq!(adaptive_step(64.0, 4), TimeMus::new(1, 1));
    assert_eq!(snap_step(64.0, 4), TimeMus::new(1, 1));
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
        snap.snap(five_sixteenths, 64.0, 4, false),
        TimeMus::new(1, 2)
    );
    assert_eq!(snap.step(64.0, 4), TimeMus::new(1, 2));
    assert_eq!(snap_step_label(snap.step(64.0, 4), 4), "1/8");
}

#[test]
fn disabling_snap_and_modifier_bypass_return_raw_times() {
    let snap = SnapSettings::default();
    let time = TimeMus::new(37, 16);
    assert_eq!(snap.snap(time, 64.0, 4, false), TimeMus::new(2, 1));
    assert_eq!(snap.snap(time, 64.0, 4, true), time);
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
    assert_eq!(disabled.snap(time, 64.0, 4, false), time);
    assert!(disabled.bypassed(Modifiers::NONE));
}

#[test]
fn fine_manual_levels_do_not_explode_the_visible_grid() {
    let snap = SnapSettings {
        enabled: true,
        adaptive: false,
        level: SnapLevel::new(1, 65536),
    };
    let display = snap.display_step(64.0, 4);
    let pixels = display.as_f64() as f32 * 64.0;
    assert!(
        pixels >= 40.0 - 1e-3,
        "display step {display} is only {pixels} px wide"
    );
    // Snapping itself still uses the exact fine level.
    assert_eq!(snap.step(64.0, 4), SnapLevel::new(1, 65536).step());
    assert_eq!(snap_step_label(snap.step(64.0, 4), 4), "1/65536");
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
