use egui::{Pos2, Rect};
use notwig_core::{RangeMus, TimeMus};
use notwig_ui::test_api::timeline::TimelineGrid;
use notwig_ui::test_api::timeline::clip::{average_peak, peak_index, waveform_region};
use notwig_ui::test_api::timeline::{pointer_delta, selection_range, snapped_delta};

#[test]
fn coordinates_round_trip_and_snap() {
    let grid = TimelineGrid::new(TimeMus::zero(), 64.0, 4);
    let x = grid.x_for_beat(TimeMus::new(2, 1), 10.0);
    assert!((x - 138.0).abs() < 0.01);
    assert_eq!(grid.beat_for_x(x, 10.0), TimeMus::new(2, 1));
    assert_eq!(grid.snap(TimeMus::new(9, 32), false), TimeMus::new(1, 4));
}

#[test]
fn waveform_source_samples_map_to_asset_peaks_without_stretching() {
    assert_eq!(peak_index(0.0, 1_000, 10), 0);
    assert_eq!(peak_index(500.0, 1_000, 10), 5);
    assert_eq!(peak_index(750.0, 1_000, 10), 7);
    assert_eq!(peak_index(999.0, 1_000, 10), 9);
}

#[test]
fn waveform_step_averages_peaks_in_its_source_range() {
    let peaks = [0.0, 0.5, 1.0, 0.0];
    assert_eq!(average_peak(&peaks, 0, 2), 0.5);
    assert_eq!(average_peak(&peaks, 1, 1), 0.5);
    assert_eq!(average_peak(&peaks, 2, 3), 0.5);
    assert_eq!(average_peak(&[], 0, 0), 0.0);
}

#[test]
fn over_trimmed_waveform_only_draws_available_source() {
    let full = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 10.0));
    let (rect, start, end) = waveform_region(full, full, 100, 0, 200).unwrap();
    assert!((rect.left() - 0.0).abs() < 0.01);
    assert!((rect.right() - 50.0).abs() < 0.01);
    assert_eq!(start, 0.0);
    assert_eq!(end, 100.0);
}

#[test]
fn clipped_waveform_uses_the_visible_source_window() {
    let full = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 10.0));
    let clip = Rect::from_min_max(egui::pos2(50.0, 0.0), egui::pos2(100.0, 10.0));
    let (rect, start, end) = waveform_region(full, clip, 200, 0, 200).unwrap();
    assert!((rect.left() - 50.0).abs() < 0.01);
    assert!((rect.right() - 100.0).abs() < 0.01);
    assert_eq!(start, 100.0);
    assert_eq!(end, 200.0);
}

#[test]
fn pointer_delta_tracks_the_press_origin() {
    let start = Pos2::new(10.0, 0.0);
    assert!((pointer_delta(start, Pos2::new(90.0, 0.0), 80.0) - 1.0).abs() < 1e-9);
}

#[test]
fn snapped_delta_rounds_to_the_grid() {
    let anchor = TimeMus::new(1, 1);
    let delta = snapped_delta(anchor, 0.13, TimeMus::new(1, 4), false);
    assert_eq!(delta, TimeMus::new(1, 4));
}

#[test]
fn selection_range_is_ordered_and_rejects_empty() {
    assert_eq!(
        selection_range(TimeMus::new(2, 1), TimeMus::new(1, 1)),
        RangeMus::new(TimeMus::new(1, 1), TimeMus::new(2, 1)).ok()
    );
    assert_eq!(
        selection_range(TimeMus::new(1, 1), TimeMus::new(1, 1)),
        None
    );
}
