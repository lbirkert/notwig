use std::sync::Arc;

use egui::{Rect, pos2};
use notwig_core::{AudioAssetBuffer, Curve, CurvePoint, SampleRate, TempoMap, TimeMus};
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{
    draw_waveform, draw_waveform_with_gain, level_for, mean_at, mean_level_for, min_max,
    min_max_at, sample, uses_sample_graph,
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
fn waveform_mode_uses_envelope_above_the_sample_graph_threshold() {
    assert!(
        !uses_sample_graph(256.1),
        "above the sample-graph threshold the pyramid envelope takes over"
    );
    assert!(
        !uses_sample_graph(1_000.0),
        "coarser zoom should stay on the pyramid envelope"
    );
}

#[test]
fn waveform_mode_starts_the_sample_graph_at_the_close_threshold() {
    assert!(
        uses_sample_graph(0.5),
        "sub-pixel sample spacing should show individual samples"
    );
    assert!(
        uses_sample_graph(8.0),
        "the sample graph should engage before individual samples are visible"
    );
    assert!(
        uses_sample_graph(64.0),
        "close rendering should engage sooner than the baseline bucket"
    );
    assert!(
        uses_sample_graph(255.9),
        "just below the close threshold still uses the sample graph"
    );
    assert!(
        uses_sample_graph(256.0),
        "the sample-graph boundary is inclusive"
    );
}

#[test]
fn sample_graph_switches_between_cached_mean_levels() {
    let waveform = AudioWaveform::from_samples(
        (0..4_096)
            .map(|index| (index as f32 * 0.05).sin())
            .collect::<Vec<f32>>(),
    );
    assert_eq!(mean_level_for(&waveform, 8.0), 8);
    assert_eq!(mean_level_for(&waveform, 100.0), 64);
    assert_eq!(mean_level_for(&waveform, 200.0), 128);
    assert_eq!(mean_level_for(&waveform, 256.0), 256);
}

#[test]
fn cached_mean_matches_the_exact_window_mean() {
    let samples: Vec<f32> = (0..512).map(|index| index as f32 / 512.0).collect();
    let waveform = AudioWaveform::from_samples(samples);
    let expected = (0..8).map(|index| index as f32 / 512.0).sum::<f32>() / 8.0;
    let actual = mean_at(&waveform, 8.0, 0).expect("bucket 8 mean");
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
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

#[test]
fn close_zoom_sample_graph_draws_only_the_area() {
    let waveform = AudioWaveform::from_samples(vec![0.5; 51_200]);
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
            draw_waveform(&painter, clip, clip, &waveform, 0, 51_200);
        },
    );
    assert!(
        output.shapes.iter().any(|clipped| has_mesh(&clipped.shape)),
        "close rendering should fill the area"
    );
    assert!(
        !output.shapes.iter().any(|clipped| has_path(&clipped.shape)),
        "close rendering should only draw the area, without a stroke over it"
    );
}

fn has_mesh(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Mesh(_) => true,
        egui::Shape::Vec(shapes) => shapes.iter().any(has_mesh),
        _ => false,
    }
}

fn has_path(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Path(_) => true,
        egui::Shape::Vec(shapes) => shapes.iter().any(has_path),
        _ => false,
    }
}

#[test]
fn sample_graph_is_adaptive_at_the_envelope_boundary() {
    let waveform = AudioWaveform::from_samples(vec![0.5; 51_200]);
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
            draw_waveform(&painter, clip, clip, &waveform, 0, 51_200);
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
    assert!(
        indices < 20_000,
        "the sample graph should adapt to pixels at the boundary, built {indices} indices"
    );
}

fn mesh_y_extents(output: &egui::FullOutput) -> Option<(f32, f32)> {
    fn visit(shape: &egui::Shape, min_y: &mut f32, max_y: &mut f32, found: &mut bool) {
        match shape {
            egui::Shape::Mesh(mesh) => {
                for vertex in &mesh.vertices {
                    *min_y = min_y.min(vertex.pos.y);
                    *max_y = max_y.max(vertex.pos.y);
                    *found = true;
                }
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, min_y, max_y, found);
                }
            }
            _ => {}
        }
    }
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut found = false;
    for clipped in &output.shapes {
        visit(&clipped.shape, &mut min_y, &mut max_y, &mut found);
    }
    found.then_some((min_y, max_y))
}

#[test]
fn gain_curve_scales_the_rendered_waveform() {
    let waveform = AudioWaveform::from_samples(vec![0.8; 1_000]);
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1000.0, 100.0));
    let curve = Curve::from_points([CurvePoint::new(TimeMus::zero(), -6.0)]).unwrap();
    let map = TempoMap {
        tempo_bpm: 120.0,
        sample_rate: SampleRate(48_000),
    };
    let render = |gain: bool| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 100.0));
                let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), rect);
                if gain {
                    draw_waveform_with_gain(
                        &painter,
                        rect,
                        rect,
                        &waveform,
                        0,
                        1_000,
                        &curve,
                        map,
                        TimeMus::zero(),
                    );
                } else {
                    draw_waveform(&painter, rect, rect, &waveform, 0, 1_000);
                }
            },
        )
    };
    let plain = mesh_y_extents(&render(false)).expect("plain waveform mesh");
    let scaled = mesh_y_extents(&render(true)).expect("gain-scaled waveform mesh");
    assert!(
        scaled.0 > plain.0 + 5.0,
        "a -6 dB gain curve should visibly shrink the waveform: plain={plain:?} scaled={scaled:?}"
    );
}
