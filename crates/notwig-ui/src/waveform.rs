use std::collections::BTreeMap;
use std::sync::Arc;

use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, Mesh, Painter, Rect, Shape, Stroke, pos2};
use notwig_core::{AssetId, AudioAssetBuffer, Curve, Db, TempoMap, TimeMus};
use notwig_ext_api::AudioWaveformColumn;

pub type WaveformCache = BTreeMap<AssetId, AudioWaveform>;

/// The peak hierarchy starts with buckets of four samples and grows by 4x.
const FAR_BASE_BUCKET: u64 = 4;
/// The far renderer switches from the filled sample graph to positive/negative
/// peak lines above this many samples per pixel.
const FAR_SAMPLES_PER_PIXEL: f64 = 4_096.0;
/// Aim for a peak window of roughly a quarter pixel so far lines keep more detail.
const FAR_WINDOW_DIVISOR: f64 = 4.0;
const WAVEFORM: Color32 = Color32::BLACK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaveformMode {
    SampleGraph,
    FarLines,
}

fn waveform_mode(samples_per_pixel: f64) -> WaveformMode {
    if samples_per_pixel <= FAR_SAMPLES_PER_PIXEL {
        WaveformMode::SampleGraph
    } else {
        WaveformMode::FarLines
    }
}

/// Gain curve applied to the waveform for display.
#[derive(Clone, Copy)]
pub(crate) struct WaveformGain<'a> {
    pub(crate) curve: &'a Curve,
    pub(crate) map: TempoMap,
    /// Clip-local time at the first source frame passed to the renderer.
    pub(crate) local_start: TimeMus,
}

impl WaveformGain<'_> {
    fn gain_at(&self, source_start: u64, frame: u64) -> f32 {
        let delta = frame.saturating_sub(source_start);
        let local = self.local_start + self.map.samples_to_mus(delta);
        let db = self.curve.value_at(local, 0.0);
        Db(db as f32).to_gain().0
    }

    pub(crate) fn gain_at_frame(&self, source_start: u64, frame: f64) -> f32 {
        let frame = frame.round().max(0.0) as u64;
        self.gain_at(source_start, frame)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioWaveform {
    audio: Arc<AudioAssetBuffer>,
    peak_levels: Arc<[PeakLevel]>,
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
        let peak_levels = build_peak_levels(&audio);
        Self {
            audio,
            peak_levels: peak_levels.into(),
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

    fn far_level_for(&self, samples_per_pixel: f64) -> Option<&PeakLevel> {
        let target = (samples_per_pixel / FAR_WINDOW_DIVISOR).max(FAR_BASE_BUCKET as f64);
        let index = self
            .peak_levels
            .partition_point(|level| level.bucket as f64 <= target)
            .saturating_sub(1);
        self.peak_levels
            .get(index)
            .or_else(|| self.peak_levels.first())
    }

    /// Returns the exact positive and negative peaks in a fractional sample span.
    fn range_peak(&self, start: f64, end: f64) -> (f32, f32) {
        let frames = self.audio.frame_count();
        if frames == 0 {
            return (0.0, 0.0);
        }
        if end <= start {
            let sample = self.sample(start.max(0.0) as u64);
            return (sample, sample);
        }
        let first = (start.floor().max(0.0) as u64).min(frames - 1);
        let last = (end.ceil().max(0.0) as u64).clamp(first.saturating_add(1), frames);
        self.range_peak_frames(first, last)
    }

    /// Queries exact peaks by consuming the largest aligned 4^n buckets first,
    /// then progressively smaller buckets until the span is covered.
    fn range_peak_frames(&self, start: u64, end: u64) -> (f32, f32) {
        let frames = self.audio.frame_count();
        if frames == 0 {
            return (0.0, 0.0);
        }
        let mut position = start.min(frames);
        let end = end.min(frames);
        if position >= end {
            let sample = self.sample(position.min(frames - 1));
            return (sample, sample);
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        while position < end {
            let remaining = end - position;
            let max_level = ((remaining.ilog2() / 2) as usize).min(self.peak_levels.len());
            let alignment = if position == 0 {
                usize::MAX
            } else {
                (position.trailing_zeros() / 2) as usize
            };
            let level_index = alignment.min(max_level);
            if level_index == 0 {
                let sample = self.sample(position);
                min = min.min(sample);
                max = max.max(sample);
                position += 1;
            } else {
                let level = &self.peak_levels[level_index - 1];
                let index = (position / level.bucket) as usize;
                if let (Some(&low), Some(&high)) = (level.min.get(index), level.max.get(index)) {
                    min = min.min(low);
                    max = max.max(high);
                }
                position = position.saturating_add(level.bucket);
            }
        }
        if min.is_finite() && max.is_finite() {
            (min, max)
        } else {
            (0.0, 0.0)
        }
    }

    /// Builds one min/max column per panel point for a source range.
    pub(crate) fn column_peaks(
        &self,
        source_start: u64,
        source_end: u64,
        columns: usize,
    ) -> Vec<AudioWaveformColumn> {
        let mut result = vec![AudioWaveformColumn::default(); columns];
        if columns == 0 || source_end <= source_start {
            return result;
        }
        let frames = self.frame_count();
        let start = source_start.min(frames);
        let end = source_end.min(frames).max(start);
        if end <= start {
            return result;
        }
        let samples_per_column = (end - start) as f64 / columns as f64;
        if samples_per_column >= FAR_BASE_BUCKET as f64 {
            let Some(level) = self.far_level_for(samples_per_column) else {
                return result;
            };
            let bucket = level.bucket;
            for (column, slot) in result.iter_mut().enumerate() {
                let column_start = start as f64 + column as f64 * samples_per_column;
                let column_end =
                    (start as f64 + (column + 1) as f64 * samples_per_column).min(end as f64);
                if column_end <= column_start {
                    continue;
                }
                let first = (column_start.floor() as u64 / bucket) as usize;
                let last = (((column_end.ceil() as u64).saturating_sub(1)) / bucket) as usize;
                let mut min = 0.0_f32;
                let mut max = 0.0_f32;
                let mut any = false;
                for index in first..=last {
                    let (Some(&low), Some(&high)) = (level.min.get(index), level.max.get(index))
                    else {
                        continue;
                    };
                    if any {
                        min = min.min(low);
                        max = max.max(high);
                    } else {
                        min = low;
                        max = high;
                        any = true;
                    }
                }
                if any {
                    *slot = AudioWaveformColumn {
                        min: min.clamp(-1.0, 1.0),
                        max: max.clamp(-1.0, 1.0),
                    };
                }
            }
        } else {
            for (column, slot) in result.iter_mut().enumerate() {
                let column_start = start + (column as f64 * samples_per_column).floor() as u64;
                let column_end = (start + ((column + 1) as f64 * samples_per_column).ceil() as u64)
                    .min(end)
                    .max(column_start.saturating_add(1))
                    .min(end);
                let mut min = 0.0_f32;
                let mut max = 0.0_f32;
                let mut any = false;
                for frame in column_start..column_end {
                    let value = self.sample(frame);
                    if any {
                        min = min.min(value);
                        max = max.max(value);
                    } else {
                        min = value;
                        max = value;
                        any = true;
                    }
                }
                if any {
                    *slot = AudioWaveformColumn {
                        min: min.clamp(-1.0, 1.0),
                        max: max.clamp(-1.0, 1.0),
                    };
                }
            }
        }
        result
    }
}

fn build_peak_levels(audio: &AudioAssetBuffer) -> Vec<PeakLevel> {
    let frames = audio.frame_count();
    if frames == 0 {
        return Vec::new();
    }
    let channels = usize::from(audio.channels.max(1));
    let count = frames.div_ceil(FAR_BASE_BUCKET) as usize;
    let mut min = vec![f32::INFINITY; count];
    let mut max = vec![f32::NEG_INFINITY; count];
    for frame in 0..frames {
        let base = frame as usize * channels;
        let mut sum = 0.0_f32;
        for channel in 0..channels {
            sum += audio.samples.get(base + channel).copied().unwrap_or(0.0);
        }
        let sample = sum / channels as f32;
        let bucket = (frame / FAR_BASE_BUCKET) as usize;
        min[bucket] = min[bucket].min(sample);
        max[bucket] = max[bucket].max(sample);
    }
    let mut bucket = FAR_BASE_BUCKET;
    let mut levels = Vec::new();
    loop {
        let next = (min.len() > 1).then(|| combine_peak_levels(&min, &max));
        levels.push(PeakLevel { bucket, min, max });
        let Some((next_min, next_max)) = next else {
            break;
        };
        min = next_min;
        max = next_max;
        bucket = bucket.saturating_mul(4);
    }
    levels
}

fn combine_peak_levels(min: &[f32], max: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let count = min.len().div_ceil(4);
    let mut next_min = vec![f32::INFINITY; count];
    let mut next_max = vec![f32::NEG_INFINITY; count];
    for (index, (low, high)) in min.iter().zip(max).enumerate() {
        let parent = index / 4;
        next_min[parent] = next_min[parent].min(*low);
        next_max[parent] = next_max[parent].max(*high);
    }
    (next_min, next_max)
}

pub(crate) fn draw_waveform(
    painter: &Painter,
    full_rect: Rect,
    clip_rect: Rect,
    waveform: &AudioWaveform,
    source_start: u64,
    source_end: u64,
    gain: Option<&WaveformGain<'_>>,
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
    match waveform_mode(samples_per_pixel) {
        WaveformMode::SampleGraph => {
            draw_sample_graph(&painter, rect, waveform, start, end, source_start, gain);
        }
        WaveformMode::FarLines => {
            draw_far_lines(
                &painter,
                rect,
                waveform,
                start,
                end,
                samples_per_pixel,
                source_start,
                gain,
            );
        }
    }
    draw_zero_line(&painter, rect);
}

fn draw_zero_line(painter: &Painter, rect: Rect) {
    let y = rect.center().y;
    painter.add(Shape::line(
        vec![pos2(rect.left(), y), pos2(rect.right(), y)],
        Stroke::new(1.0_f32, WAVEFORM),
    ));
}

#[allow(clippy::too_many_arguments)]
fn draw_far_lines(
    painter: &Painter,
    rect: Rect,
    waveform: &AudioWaveform,
    start: f64,
    end: f64,
    _samples_per_pixel: f64,
    source_start: u64,
    gain: Option<&WaveformGain<'_>>,
) {
    let span = end - start;
    if span <= 0.0 {
        return;
    }
    let columns = (rect.width() * FAR_WINDOW_DIVISOR as f32).ceil().max(2.0) as usize;
    let step = rect.width() / columns as f32;
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(columns * 8);
    mesh.reserve_triangles(columns * 4);
    for column in 0..columns {
        let x0 = rect.left() + column as f32 * step;
        let x1 = if column + 1 == columns {
            rect.right()
        } else {
            x0 + step
        };
        let t0 = (x0 - rect.left()) / rect.width();
        let t1 = (x1 - rect.left()) / rect.width();
        let (mut min, mut max) = waveform.range_peak(
            start + f64::from(t0) * span,
            start + f64::from(t1) * span,
        );
        if let Some(gain) = gain {
            let frame = start + f64::from(f32::midpoint(t0, t1)) * span;
            let scale = gain.gain_at_frame(source_start, frame);
            min *= scale;
            max *= scale;
        }
        let x = f32::midpoint(x0, x1);
        let positive = center - max.max(0.0).clamp(-1.0, 1.0) * half;
        let negative = center - min.min(0.0).clamp(-1.0, 1.0) * half;
        push_vertical_line(&mut mesh, x, center, positive, WAVEFORM);
        push_vertical_line(&mut mesh, x, center, negative, WAVEFORM);
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn push_vertical_line(mesh: &mut Mesh, x: f32, from: f32, to: f32, color: Color32) {
    let (top, bottom) = if from <= to { (from, to) } else { (to, from) };
    if bottom - top < 0.5 {
        return;
    }
    let left = x - 0.5;
    let right = x + 0.5;
    let base = mesh.vertices.len() as u32;
    for pos in [
        pos2(left, top),
        pos2(right, top),
        pos2(right, bottom),
        pos2(left, bottom),
    ] {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

#[allow(clippy::too_many_arguments)]
fn draw_sample_graph(
    painter: &Painter,
    rect: Rect,
    waveform: &AudioWaveform,
    start: f64,
    end: f64,
    source_start: u64,
    gain: Option<&WaveformGain<'_>>,
) {
    let frames = waveform.frame_count();
    let span = end - start;
    if span <= 0.0 {
        return;
    }
    let first = (start.floor().max(0.0) as u64).min(frames - 1);
    let last = (end.ceil().max(0.0) as u64).min(frames - 1).max(first);
    if last <= first {
        return;
    }
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let samples_per_pixel = span / f64::from(rect.width().max(1.0));
    let mut points = Vec::new();
    if samples_per_pixel < 1.0 {
        let mut frame = first;
        loop {
            let mut value = waveform.sample(frame);
            if let Some(gain) = gain {
                value *= gain.gain_at(source_start, frame);
            }
            let x = rect.left() + ((frame as f64 - start) / span) as f32 * rect.width();
            points.push(pos2(x, center - value.clamp(-1.0, 1.0) * half));
            if frame >= last {
                break;
            }
            frame += 1;
        }
    } else {
        let columns = rect.width().ceil().max(1.0) as usize;
        let step = rect.width() / columns as f32;
        points.reserve(columns);
        for column in 0..columns {
            let x0 = rect.left() + column as f32 * step;
            let x1 = if column + 1 == columns {
                rect.right()
            } else {
                x0 + step
            };
            let t0 = (x0 - rect.left()) / rect.width();
            let t1 = (x1 - rect.left()) / rect.width();
            let (mut min, mut max) = waveform.range_peak(
                start + f64::from(t0) * span,
                start + f64::from(t1) * span,
            );
            if let Some(gain) = gain {
                let frame = start + f64::from(f32::midpoint(t0, t1)) * span;
                let scale = gain.gain_at_frame(source_start, frame);
                min *= scale;
                max *= scale;
            }
            let value = if max.abs() >= min.abs() { max } else { min };
            points.push(pos2(
                f32::midpoint(x0, x1),
                center - value.clamp(-1.0, 1.0) * half,
            ));
        }
    }
    crate::widgets::area::fill_polyline_area(painter, &points, center, WAVEFORM);
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

    pub fn range_peak(waveform: &AudioWaveform, start: u64, end: u64) -> (f32, f32) {
        waveform.range_peak_frames(start, end)
    }

    pub fn peak_level_blocks(waveform: &AudioWaveform) -> Vec<u64> {
        waveform
            .peak_levels
            .iter()
            .map(|level| level.bucket)
            .collect()
    }

    pub fn uses_far_lines(samples_per_pixel: f64) -> bool {
        super::waveform_mode(samples_per_pixel) == super::WaveformMode::FarLines
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

    pub fn draw_waveform_with_gain(
        painter: &Painter,
        full_rect: Rect,
        clip_rect: Rect,
        waveform: &AudioWaveform,
        source_start: u64,
        source_end: u64,
        curve: &Curve,
        map: TempoMap,
        local_start: TimeMus,
    ) {
        let gain = WaveformGain {
            curve,
            map,
            local_start,
        };
        super::draw_waveform(
            painter,
            full_rect,
            clip_rect,
            waveform,
            source_start,
            source_end,
            Some(&gain),
        );
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
            None,
        );
    }
}
