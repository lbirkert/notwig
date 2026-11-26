use egui::{Rect, pos2};
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{
    draw_waveform, level_for, uses_sample_graph, window_peak_level_for,
};

fn dense_samples(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| if index % 2 == 0 { -0.5 } else { 0.5 })
        .collect()
}

#[test]
fn close_threshold_starts_the_sample_graph_and_pyramid_envelope() {
    let waveform = AudioWaveform::from_samples(dense_samples(65_536));
    for samples_per_pixel in [
        0.1, 0.5, 1.0, 8.0, 32.0, 48.0, 63.9, 64.0, 128.0, 255.9, 256.0,
    ] {
        assert!(
            uses_sample_graph(samples_per_pixel),
            "samples_per_pixel={samples_per_pixel} should use the sample graph"
        );
    }
    for samples_per_pixel in [256.1, 512.0, 4096.0] {
        assert!(
            !uses_sample_graph(samples_per_pixel),
            "samples_per_pixel={samples_per_pixel} should use the pyramid envelope"
        );
    }
    assert_eq!(level_for(&waveform, 64.0), 64);
    assert_eq!(level_for(&waveform, 128.0), 128);
    assert_eq!(level_for(&waveform, 4096.0), 4096);
    assert_eq!(window_peak_level_for(&waveform, 64.0), 64);
    assert_eq!(window_peak_level_for(&waveform, 200.0), 128);
    assert_eq!(window_peak_level_for(&waveform, 256.0), 256);
}

fn mesh_indices(output: &egui::FullOutput) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Mesh(mesh) => mesh.indices.len(),
            egui::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            _ => 0,
        }
    }
    output
        .shapes
        .iter()
        .map(|clipped| count(&clipped.shape))
        .sum()
}

#[test]
fn dense_sample_graph_below_the_threshold_bounds_geometry() {
    let waveform = AudioWaveform::from_samples(vec![0.5; 1_000_000]);
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
            let full = Rect::from_min_max(pos2(0.0, 40.0), pos2(10_000.0, 140.0));
            draw_waveform(&painter, full, clip, &waveform, 0, 1_000_000);
        },
    );

    assert_eq!(
        window_peak_level_for(&waveform, 100.0),
        64,
        "the sample graph should read a cached peak level below the threshold"
    );
    let indices = mesh_indices(&output);
    assert!(indices > 0, "the waveform should draw something");
    assert!(
        indices < 10_000,
        "dense waveform built {indices} indices; it should scale with pixels"
    );
}

#[test]
fn wide_span_above_the_threshold_uses_the_area_envelope() {
    let waveform = AudioWaveform::from_samples(dense_samples(1_000_000));
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1600.0, 200.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            let clip = Rect::from_min_max(pos2(0.0, 40.0), pos2(1600.0, 140.0));
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), clip);
            let full = Rect::from_min_max(pos2(0.0, 40.0), pos2(1600.0, 140.0));
            draw_waveform(&painter, full, clip, &waveform, 0, 800_000);
        },
    );

    let indices = mesh_indices(&output);
    assert!(indices > 0, "the waveform should draw something");
    assert!(
        indices < 10_000,
        "wide view built {indices} indices; it should use the envelope"
    );
}
