use std::sync::Arc;

use egui::{Rect, pos2};
use notwig_core::AudioAssetBuffer;
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{
    draw_waveform, level_for, min_max, min_max_at, sample, uses_sample_graph,
};

#[test]
fn raw_min_max_keeps_signed_samples() {
    let waveform = AudioWaveform::from_samples(vec![-0.5, 0.75, -0.25, 0.0]);
    assert_eq!(waveform.frame_count(), 4);
    assert_eq!(min_max(&waveform, 0, 4), (-0.5, 0.75));
    assert_eq!(min_max(&waveform, 1, 2), (0.75, 0.75));
}

#[test]
fn stereo_samples_are_downmixed_for_display() {
    let waveform = AudioWaveform::new(Arc::new(AudioAssetBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: vec![1.0, -1.0, 0.5, 0.25],
    }));
    assert_eq!(sample(&waveform, 0), 0.0);
    assert_eq!(sample(&waveform, 1), 0.375);
}

#[test]
fn pyramid_levels_match_exact_ranges_at_their_bucket_size() {
    let samples: Vec<f32> = (0..1024)
        .map(|index| ((index as f32) * 0.017).sin())
        .collect();
    let waveform = AudioWaveform::from_samples(samples);
    assert_eq!(level_for(&waveform, 64.0), 64);
    assert_eq!(level_for(&waveform, 127.0), 64);
    assert_eq!(level_for(&waveform, 128.0), 128);
    assert_eq!(level_for(&waveform, 1024.0), 1024);
    assert_eq!(
        min_max_at(&waveform, 0, 1024, 64.0),
        min_max(&waveform, 0, 1024)
    );
    assert_eq!(
        min_max_at(&waveform, 0, 1024, 1024.0),
        min_max(&waveform, 0, 1024)
    );
}

#[test]
fn waveform_mode_uses_envelope_above_the_baseline_bucket() {
    assert!(
        !uses_sample_graph(64.1),
        "more than one baseline bucket per pixel should use the pyramid envelope"
    );
    assert!(
        !uses_sample_graph(128.0),
        "coarser zoom should stay on the pyramid envelope"
    );
    assert!(
        !uses_sample_graph(1_000.0),
        "very coarse zoom should stay on the pyramid envelope"
    );
}

#[test]
fn waveform_mode_starts_the_sample_graph_at_the_baseline_bucket() {
    assert!(
        uses_sample_graph(0.5),
        "sub-pixel sample spacing should show individual samples"
    );
    assert!(
        uses_sample_graph(8.0),
        "the intermediate envelope stage is gone below the baseline bucket"
    );
    assert!(
        uses_sample_graph(63.9),
        "just below the baseline bucket still uses the sample graph"
    );
    assert!(
        uses_sample_graph(64.0),
        "the sample-graph boundary is inclusive"
    );
}

#[test]
fn drawing_a_huge_clip_only_builds_geometry_for_the_visible_window() {
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 200.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            let clip = Rect::from_min_max(pos2(0.0, 40.0), pos2(800.0, 140.0));
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), clip);
            let waveform = AudioWaveform::from_samples(
                (0..1_000_000)
                    .map(|index| if index % 2 == 0 { 0.5 } else { -0.5 })
                    .collect::<Vec<f32>>(),
            );
            let full = Rect::from_min_max(pos2(-5_000_000.0, 40.0), pos2(5_000_000.0, 140.0));
            draw_waveform(&painter, full, full, &waveform, 0, 1_000_000);
        },
    );
    let indices: usize = output
        .shapes
        .iter()
        .map(|clipped| match &clipped.shape {
            egui::Shape::Mesh(mesh) => mesh.indices.len(),
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .map(|shape| match shape {
                    egui::Shape::Mesh(mesh) => mesh.indices.len(),
                    _ => 0,
                })
                .sum(),
            _ => 0,
        })
        .sum();
    assert!(indices > 0);
    assert!(indices < 10_000, "waveform built {indices} indices");
}

#[test]
fn close_zoom_sample_graph_still_fills_the_area() {
    let waveform = AudioWaveform::from_samples(
        (0..4_000)
            .map(|index| if index % 2 == 0 { 0.5 } else { -0.5 })
            .collect::<Vec<f32>>(),
    );
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 200.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            let clip = Rect::from_min_max(pos2(0.0, 40.0), pos2(800.0, 140.0));
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), clip);
            let full = Rect::from_min_max(pos2(0.0, 40.0), pos2(4_000.0, 140.0));
            draw_waveform(&painter, full, clip, &waveform, 0, 4_000);
        },
    );
    let indices: usize = output
        .shapes
        .iter()
        .map(|clipped| match &clipped.shape {
            egui::Shape::Mesh(mesh) => mesh.indices.len(),
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .map(|shape| match shape {
                    egui::Shape::Mesh(mesh) => mesh.indices.len(),
                    _ => 0,
                })
                .sum(),
            _ => 0,
        })
        .sum();
    assert!(
        indices > 0,
        "close-zoom sample graph must fill the area, not only draw a line"
    );
}
