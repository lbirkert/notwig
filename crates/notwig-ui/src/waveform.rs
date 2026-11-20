use std::collections::BTreeMap;
use std::sync::Arc;

use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, Mesh, Painter, Rect, Shape, Stroke, pos2};
use notwig_core::{AssetId, AudioAssetBuffer};

pub type WaveformCache = BTreeMap<AssetId, AudioWaveform>;

const BASE_BUCKET: u64 = 64;
const SAMPLE_GRAPH_SAMPLES_PER_PIXEL: f64 = 32.0;
const FILL: Color32 = Color32::from_black_alpha(132);
const LINE: Color32 = Color32::from_black_alpha(190);

#[derive(Debug, Clone, PartialEq)]
pub struct AudioWaveform {
    audio: Arc<AudioAssetBuffer>,
    levels: Arc<[PeakLevel]>,
}

#[derive(Debug, PartialEq)]
struct PeakLevel {
    bucket: u64,
    min: Vec<f32>,
    max: Vec<f32>,
}

impl AudioWaveform {
    #[must_use]
    pub fn new(audio: Arc<AudioAssetBuffer>) -> Self {
        let levels = build_levels(&audio);
        Self {
            audio,
            levels: levels.into(),
        }
    }

    #[must_use]
    pub fn from_samples(samples: impl Into<Vec<f32>>) -> Self {
        Self::new(Arc::new(AudioAssetBuffer {
            sample_rate: notwig_core::SampleRate::DEFAULT.value(),
            channels: 1,
            samples: samples.into(),
        }))
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.audio.frame_count()
    }

    fn sample(&self, frame: u64) -> f32 {
        if frame >= self.audio.frame_count() {
            return 0.0;
        }
        let channels = usize::from(self.audio.channels.max(1));
        let base = frame as usize * channels;
        let mut sum = 0.0_f32;
        for channel in 0..channels {
            sum += self
                .audio
                .samples
                .get(base + channel)
                .copied()
                .unwrap_or(0.0);
        }
        sum / channels as f32
    }

    fn level_for(&self, samples_per_pixel: f64) -> Option<&PeakLevel> {
        if samples_per_pixel < BASE_BUCKET as f64 {
            return None;
        }
        let index = self
            .levels
            .partition_point(|level| level.bucket as f64 <= samples_per_pixel)
            .saturating_sub(1);
        self.levels.get(index).or_else(|| self.levels.first())
    }

    fn min_max(&self, start: f64, end: f64, level: Option<&PeakLevel>) -> (f32, f32) {
        let frames = self.audio.frame_count();
        if frames == 0 || end <= start {
            let sample = self.sample(start.max(0.0) as u64);
            return (sample, sample);
        }
        let last = frames - 1;
        let first_frame = (start.floor().max(0.0) as u64).min(last);
        let end_frame = (end.ceil().max(0.0) as u64).clamp(first_frame.saturating_add(1), frames);
        if let Some(level) = level {
            let first_bucket = (first_frame / level.bucket) as usize;
            let last_bucket = ((end_frame - 1) / level.bucket) as usize;
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for bucket in first_bucket..=last_bucket {
                if let (Some(low), Some(high)) = (level.min.get(bucket), level.max.get(bucket)) {
                    min = min.min(*low);
                    max = max.max(*high);
                }
            }
            if min.is_finite() && max.is_finite() {
                return (min, max);
            }
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for frame in first_frame..end_frame {
            let sample = self.sample(frame);
            min = min.min(sample);
            max = max.max(sample);
        }
        if min.is_finite() && max.is_finite() {
            (min, max)
        } else {
            (0.0, 0.0)
        }
    }
}

fn build_levels(audio: &AudioAssetBuffer) -> Vec<PeakLevel> {
    let frames = audio.frame_count();
    if frames == 0 {
        return Vec::new();
    }
    let count = frames.div_ceil(BASE_BUCKET) as usize;
    let mut min = vec![f32::INFINITY; count];
    let mut max = vec![f32::NEG_INFINITY; count];
    let channels = usize::from(audio.channels.max(1));
    for frame in 0..frames {
        let base = frame as usize * channels;
        let mut sum = 0.0_f32;
        for channel in 0..channels {
            sum += audio.samples.get(base + channel).copied().unwrap_or(0.0);
        }
        let sample = sum / channels as f32;
        let bucket = (frame / BASE_BUCKET) as usize;
        min[bucket] = min[bucket].min(sample);
        max[bucket] = max[bucket].max(sample);
    }
    let mut levels = vec![PeakLevel {
        bucket: BASE_BUCKET,
        min,
        max,
    }];
    while levels.last().is_some_and(|level| level.min.len() > 1) {
        let Some(previous) = levels.last() else {
            break;
        };
        let count = previous.min.len().div_ceil(2);
        let mut min = vec![f32::INFINITY; count];
        let mut max = vec![f32::NEG_INFINITY; count];
        for (index, (low, high)) in previous.min.iter().zip(&previous.max).enumerate() {
            let bucket = index / 2;
            min[bucket] = min[bucket].min(*low);
            max[bucket] = max[bucket].max(*high);
        }
        levels.push(PeakLevel {
            bucket: previous.bucket.saturating_mul(2),
            min,
            max,
        });
    }
    levels
}

pub(crate) fn draw_waveform(
    painter: &Painter,
    full_rect: Rect,
    clip_rect: Rect,
    waveform: &AudioWaveform,
    source_start: u64,
    source_end: u64,
) {
    if waveform.frame_count() == 0 {
        return;
    }
    let clip_rect = clip_rect.intersect(painter.clip_rect());
    let Some((rect, start, end)) = waveform_region(
        full_rect,
        clip_rect,
        waveform.frame_count(),
        source_start,
        source_end,
    ) else {
        return;
    };
    let samples_per_pixel = (end - start) / f64::from(rect.width().max(1.0));
    let painter = painter.with_clip_rect(rect);
    if samples_per_pixel <= SAMPLE_GRAPH_SAMPLES_PER_PIXEL {
        draw_sample_graph(&painter, rect, waveform, start, end);
    } else {
        draw_envelope(&painter, rect, waveform, start, end, samples_per_pixel);
    }
}

fn draw_envelope(
    painter: &Painter,
    rect: Rect,
    waveform: &AudioWaveform,
    start: f64,
    end: f64,
    samples_per_pixel: f64,
) {
    let level = waveform.level_for(samples_per_pixel);
    let span = end - start;
    let columns = rect.width().ceil().max(1.0) as usize;
    let step = rect.width() / columns as f32;
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(columns * 4);
    mesh.reserve_triangles(columns * 2);
    for column in 0..columns {
        let x0 = rect.left() + column as f32 * step;
        let x1 = if column + 1 == columns {
            rect.right()
        } else {
            x0 + step
        };
        let t0 = (x0 - rect.left()) / rect.width();
        let t1 = (x1 - rect.left()) / rect.width();
        let (min, max) = waveform.min_max(
            start + f64::from(t0) * span,
            start + f64::from(t1) * span,
            level,
        );
        let top = center - max.clamp(-1.0, 1.0) * half;
        let bottom = center - min.clamp(-1.0, 1.0) * half;
        if bottom <= top {
            continue;
        }
        push_quad(
            &mut mesh,
            pos2(x0, top),
            pos2(x1, top),
            pos2(x1, bottom),
            pos2(x0, bottom),
        );
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn draw_sample_graph(
    painter: &Painter,
    rect: Rect,
    waveform: &AudioWaveform,
    start: f64,
    end: f64,
) {
    let frames = waveform.frame_count();
    let span = end - start;
    let first = (start.floor().max(0.0) as u64).min(frames - 1);
    let last = (end.ceil().max(0.0) as u64).min(frames - 1).max(first);
    if last <= first {
        return;
    }
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let mut points = Vec::with_capacity((last - first + 1) as usize);
    for frame in first..=last {
        let x = rect.left() + ((frame as f64 - start) / span) as f32 * rect.width();
        let value = waveform.sample(frame).clamp(-1.0, 1.0);
        points.push(pos2(x, center - value * half));
    }
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(points.len().saturating_sub(1) * 4);
    mesh.reserve_triangles(points.len().saturating_sub(1) * 2);
    for pair in points.windows(2) {
        add_area_segment(&mut mesh, pair[0], pair[1], center);
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
    painter.add(Shape::line(points, Stroke::new(1.0_f32, LINE)));
}

fn add_area_segment(mesh: &mut Mesh, from: egui::Pos2, to: egui::Pos2, zero: f32) {
    let from_side = from.y - zero;
    let to_side = to.y - zero;
    if from_side == 0.0 && to_side == 0.0 {
        return;
    }
    if from_side.signum() == to_side.signum() || to_side == 0.0 {
        push_quad(mesh, pos2(from.x, zero), pos2(to.x, zero), to, from);
        return;
    }
    if from_side == 0.0 {
        push_triangle(mesh, pos2(from.x, zero), pos2(to.x, zero), to);
        return;
    }
    let crossing = (zero - from.y) / (to.y - from.y);
    let x = from.x + (to.x - from.x) * crossing.clamp(0.0, 1.0);
    push_triangle(mesh, pos2(from.x, zero), pos2(x, zero), from);
    push_triangle(mesh, pos2(x, zero), pos2(to.x, zero), to);
}

fn push_triangle(mesh: &mut Mesh, a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) {
    let base = mesh.vertices.len() as u32;
    for pos in [a, b, c] {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: FILL,
        });
    }
    mesh.add_triangle(base, base + 1, base + 2);
}

fn push_quad(mesh: &mut Mesh, a: egui::Pos2, b: egui::Pos2, c: egui::Pos2, d: egui::Pos2) {
    let base = mesh.vertices.len() as u32;
    for pos in [a, b, c, d] {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: FILL,
        });
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

pub(crate) fn waveform_region(
    full_rect: Rect,
    clip_rect: Rect,
    asset_samples: u64,
    source_start: u64,
    source_end: u64,
) -> Option<(Rect, f64, f64)> {
    if full_rect.width() <= 0.0 || source_end <= source_start {
        return None;
    }
    let visible = full_rect.intersect(clip_rect);
    if visible.width() < 1.0 {
        return None;
    }
    let to_source = |x: f32| {
        let t = f64::from(((x - full_rect.left()) / full_rect.width()).clamp(0.0, 1.0));
        source_start as f64 + t * (source_end - source_start) as f64
    };
    let visible_start = to_source(visible.left());
    let visible_end = to_source(visible.right());
    let span = visible_end - visible_start;
    if span <= 0.0 {
        return None;
    }
    let available_start = visible_start.clamp(0.0, asset_samples as f64);
    let available_end = visible_end.clamp(0.0, asset_samples as f64);
    if available_end <= available_start {
        return None;
    }
    let u0 = ((available_start - visible_start) / span) as f32;
    let u1 = ((available_end - visible_start) / span) as f32;
    let rect = Rect::from_min_max(
        egui::pos2(visible.left() + u0 * visible.width(), visible.top()),
        egui::pos2(visible.left() + u1 * visible.width(), visible.bottom()),
    );
    Some((rect, available_start, available_end))
}

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn min_max(waveform: &AudioWaveform, start: u64, end: u64) -> (f32, f32) {
        waveform.min_max(start as f64, end as f64, None)
    }

    pub fn min_max_at(
        waveform: &AudioWaveform,
        start: u64,
        end: u64,
        samples_per_pixel: f64,
    ) -> (f32, f32) {
        waveform.min_max(
            start as f64,
            end as f64,
            waveform.level_for(samples_per_pixel),
        )
    }

    pub fn level_for(waveform: &AudioWaveform, samples_per_pixel: f64) -> usize {
        waveform
            .level_for(samples_per_pixel)
            .map_or(0, |level| level.bucket as usize)
    }

    pub fn sample(waveform: &AudioWaveform, frame: u64) -> f32 {
        waveform.sample(frame)
    }

    pub fn waveform_region(
        full_rect: Rect,
        clip_rect: Rect,
        asset_samples: u64,
        source_start: u64,
        source_end: u64,
    ) -> Option<(Rect, f64, f64)> {
        super::waveform_region(
            full_rect,
            clip_rect,
            asset_samples,
            source_start,
            source_end,
        )
    }

    pub fn draw_waveform(
        painter: &Painter,
        full_rect: Rect,
        clip_rect: Rect,
        waveform: &AudioWaveform,
        source_start: u64,
        source_end: u64,
    ) {
        super::draw_waveform(
            painter,
            full_rect,
            clip_rect,
            waveform,
            source_start,
            source_end,
        );
    }
}
