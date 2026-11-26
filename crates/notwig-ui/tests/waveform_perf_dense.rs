use egui::{Rect, pos2};
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{draw_waveform, far_level_for, window_peak_level_for};

fn dense_samples(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| if index % 2 == 0 { -0.5 } else { 0.5 })
        .collect()
}

#[test]
fn cached_window_peaks_cover_all_zoom_levels() {
    let waveform = AudioWaveform::from_samples(dense_samples(65_536));
    assert_eq!(window_peak_level_for(&waveform, 8.0), 8);
    assert_eq!(window_peak_level_for(&waveform, 64.0), 64);
    assert_eq!(window_peak_level_for(&waveform, 200.0), 128);
    assert_eq!(window_peak_level_for(&waveform, 256.0), 256);
    assert_eq!(window_peak_level_for(&waveform, 4_096.0), 4_096);
    assert_eq!(
        window_peak_level_for(&waveform, 1_000_000.0),
        65_536,
        "far zoom should use the whole-asset peak level"
    );
    assert_eq!(far_level_for(&waveform, 64.0), 64);
    assert_eq!(far_level_for(&waveform, 1_000.0), 512);
    assert_eq!(
        far_level_for(&waveform, 1_000_000.0),
        65_536,
        "far lines should use the whole-asset peak level"
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

fn path_point_count(output: &egui::FullOutput) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Path(path) => path.points.len(),
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

    assert_eq!(
        window_peak_level_for(&waveform, 100.0),
        64,
        "the sample graph should read a cached peak level"
    );
    let indices = mesh_indices(&output);
    assert!(indices > 0, "the waveform should draw something");
    assert!(
        indices < 10_000,
        "dense waveform built {indices} indices; it should scale with pixels"
    );
}

#[test]
fn far_zoom_line_envelope_bounds_geometry() {
    let waveform = AudioWaveform::from_samples(dense_samples(1_000_000));
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
            let full = Rect::from_min_max(pos2(0.0, 40.0), pos2(800.0, 140.0));
            draw_waveform(&painter, full, clip, &waveform, 0, 800_000);
        },
    );

    assert_eq!(
        far_level_for(&waveform, 1_000.0),
        512,
        "far zoom should use a coarse cached peak level"
    );
    assert_eq!(
        mesh_indices(&output),
        0,
        "far rendering should not fill an area"
    );
    let points = path_point_count(&output);
    assert!(points > 0, "the waveform should draw peak lines");
    assert!(
        points <= 2 * 801,
        "far view built {points} line points; it should scale with pixels"
    );
}
