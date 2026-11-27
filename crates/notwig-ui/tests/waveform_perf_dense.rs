use egui::{Rect, pos2};
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{draw_waveform, peak_level_blocks};

fn dense_samples(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| if index % 2 == 0 { -0.5 } else { 0.5 })
        .collect()
}

#[test]
fn peak_hierarchy_uses_power_of_four_blocks() {
    let waveform = AudioWaveform::from_samples(dense_samples(65_536));
    assert_eq!(
        peak_level_blocks(&waveform),
        vec![4, 16, 64, 256, 1_024, 4_096, 16_384, 65_536]
    );
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
fn dense_sample_graph_bounds_geometry() {
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

    let indices = mesh_indices(&output);
    assert!(indices > 0, "the waveform should draw something");
    assert!(
        indices < 10_000,
        "dense waveform built {indices} indices; it should scale with pixels"
    );
}

#[test]
fn far_zoom_vertical_lines_bound_geometry() {
    let waveform = AudioWaveform::from_samples(dense_samples(1_000_000));
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 200.0));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            let clip = Rect::from_min_max(pos2(0.0, 40.0), pos2(200.0, 140.0));
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), clip);
            let full = Rect::from_min_max(pos2(0.0, 40.0), pos2(200.0, 140.0));
            draw_waveform(&painter, full, clip, &waveform, 0, 1_000_000);
        },
    );

    let indices = mesh_indices(&output);
    assert!(indices > 0, "the waveform should draw peak lines");
    assert!(
        indices <= 8 * 200 * 12,
        "far view built {indices} indices; it should scale with pixels"
    );
}
