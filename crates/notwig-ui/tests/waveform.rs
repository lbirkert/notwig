use std::sync::Arc;

use egui::{Rect, pos2};
use notwig_core::{AudioAssetBuffer, Curve, CurvePoint, SampleRate, TempoMap, TimeMus};
use notwig_ui::AudioWaveform;
use notwig_ui::test_api::waveform::{
    draw_waveform, draw_waveform_with_gain, far_level_for, far_peak_at, sample, uses_far_lines,
    window_peak_at, window_peak_level_for,
};

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
fn sample_graph_switches_between_cached_peak_levels() {
    let waveform = AudioWaveform::from_samples(
        (0..4_096)
            .map(|index| (index as f32 * 0.05).sin())
            .collect::<Vec<f32>>(),
    );
    assert_eq!(window_peak_level_for(&waveform, 8.0), 8);
    assert_eq!(window_peak_level_for(&waveform, 100.0), 64);
    assert_eq!(window_peak_level_for(&waveform, 200.0), 128);
    assert_eq!(window_peak_level_for(&waveform, 256.0), 256);
    assert_eq!(window_peak_level_for(&waveform, 1_024.0), 1_024);
    assert_eq!(
        window_peak_level_for(&waveform, 100_000.0),
        4_096,
        "far zoom should use the whole-asset peak level"
    );
}

#[test]
fn cached_peak_matches_the_exact_window_peak() {
    let samples: Vec<f32> = (0..512)
        .map(|index| {
            let position = index as f32 / 512.0;
            if index == 3 { -0.9 } else { position }
        })
        .collect();
    let waveform = AudioWaveform::from_samples(samples);
    let actual = window_peak_at(&waveform, 8.0, 0).expect("bucket 8 peak");
    assert_eq!(actual, -0.9);
}

#[test]
fn cached_far_peaks_keep_positive_and_negative_extremes() {
    let samples: Vec<f32> = (0..512)
        .map(|index| match index {
            3 => -0.9,
            5 => 0.8,
            _ => 0.0,
        })
        .collect();
    let waveform = AudioWaveform::from_samples(samples);
    assert_eq!(far_level_for(&waveform, 64.0), 64);
    assert_eq!(far_peak_at(&waveform, 64.0, 0), Some((-0.9, 0.8)));
}

#[test]
fn far_lines_take_over_above_the_sample_graph_threshold() {
    assert!(!uses_far_lines(0.5));
    assert!(!uses_far_lines(4_096.0));
    assert!(uses_far_lines(4_096.1));
    assert!(uses_far_lines(10_000.0));
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
fn close_zoom_sample_graph_draws_area_and_zero_line() {
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
    assert_eq!(
        path_count(&output),
        1,
        "close rendering should only add the zero line over the area"
    );
    assert!(
        has_zero_line(&output, 90.0),
        "close rendering should keep the zero line visible"
    );
}

#[test]
fn far_zoom_draws_positive_and_negative_peak_lines() {
    let waveform = AudioWaveform::from_samples(
        (0..1_000_000)
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
            let clip = Rect::from_min_max(pos2(0.0, 40.0), pos2(200.0, 140.0));
            let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), clip);
            draw_waveform(&painter, clip, clip, &waveform, 0, 1_000_000);
        },
    );
    assert_eq!(
        path_count(&output),
        1,
        "far rendering should add only the zero line as a path"
    );
    assert!(
        has_zero_line(&output, 90.0),
        "far rendering should keep the zero line visible"
    );
    let mesh = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        })
        .expect("far rendering should produce a line mesh");
    let center = 90.0;
    let mut touches_zero = false;
    let mut reaches_positive = false;
    let mut reaches_negative = false;
    for vertex in &mesh.vertices {
        if (vertex.pos.y - center).abs() < 0.01 {
            touches_zero = true;
        }
        if vertex.pos.y < center - 1.0 {
            reaches_positive = true;
        }
        if vertex.pos.y > center + 1.0 {
            reaches_negative = true;
        }
    }
    assert!(touches_zero, "far lines should start at the zero line");
    assert!(reaches_positive, "far lines should reach the positive peak");
    assert!(reaches_negative, "far lines should reach the negative peak");
}

#[test]
fn zero_line_is_visible_without_signal() {
    let waveform = AudioWaveform::from_samples(vec![0.0; 51_200]);
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
    assert_eq!(path_count(&output), 1);
    assert!(
        has_zero_line(&output, 90.0),
        "the zero line should be visible even for silent material"
    );
}

fn has_mesh(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Mesh(_) => true,
        egui::Shape::Vec(shapes) => shapes.iter().any(has_mesh),
        _ => false,
    }
}

fn path_count(output: &egui::FullOutput) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Path(_) => 1,
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

fn has_zero_line(output: &egui::FullOutput, y: f32) -> bool {
    fn visit(shape: &egui::Shape, y: f32) -> bool {
        match shape {
            egui::Shape::Path(path) => {
                path.points.len() >= 2 && path.points.iter().all(|point| (point.y - y).abs() < 0.01)
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| visit(shape, y)),
            _ => false,
        }
    }
    output.shapes.iter().any(|clipped| visit(&clipped.shape, y))
}

#[test]
fn sample_graph_is_adaptive_below_the_far_boundary() {
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
