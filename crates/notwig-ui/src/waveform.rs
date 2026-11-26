use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Color32, Painter, Rect, pos2};
use notwig_core::{AssetId, AudioAssetBuffer, Curve, Db, TempoMap, TimeMus};

pub type WaveformCache = BTreeMap<AssetId, AudioWaveform>;

/// Below this bucket the sample graph reads the decoded samples directly.
/// Cached window-peak levels start here and double to the whole asset.
const PEAK_BASE_BUCKET: u64 = 8;
const WAVEFORM: Color32 = Color32::BLACK;

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

    fn gain_at_frame(&self, source_start: u64, frame: f64) -> f32 {
        let frame = frame.round().max(0.0) as u64;
        self.gain_at(source_start, frame)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioWaveform {
    audio: Arc<AudioAssetBuffer>,
    window_peaks: Arc<[WindowPeakLevel]>,
}

#[derive(Debug, PartialEq)]
struct WindowPeakLevel {
    bucket: u64,
    peak: Vec<f32>,
}

impl AudioWaveform {
    #[must_use]
    pub fn new(audio: Arc<AudioAssetBuffer>) -> Self {
        let window_peaks = build_window_peaks(&audio);
        Self {
            audio,
            window_peaks: window_peaks.into(),
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

    fn window_peak_level_for(&self, samples_per_pixel: f64) -> Option<&WindowPeakLevel> {
        self.window_peaks
            .iter()
            .rev()
            .find(|level| level.bucket as f64 <= samples_per_pixel)
    }
}

fn build_window_peaks(audio: &AudioAssetBuffer) -> Vec<WindowPeakLevel> {
    let frames = audio.frame_count();
    if frames == 0 {
        return Vec::new();
    }
    let channels = usize::from(audio.channels.max(1));
    let count = frames.div_ceil(PEAK_BASE_BUCKET) as usize;
    let mut peaks = vec![0.0_f32; count];
    for frame in 0..frames {
        let base = frame as usize * channels;
        let mut sum = 0.0_f32;
        for channel in 0..channels {
            sum += audio.samples.get(base + channel).copied().unwrap_or(0.0);
        }
        let sample = sum / channels as f32;
        let bucket = (frame / PEAK_BASE_BUCKET) as usize;
        if sample.abs() > peaks[bucket].abs() {
            peaks[bucket] = sample;
        }
    }
    let mut bucket = PEAK_BASE_BUCKET;
    let mut levels = Vec::new();
    loop {
        let next = (peaks.len() > 1).then(|| combine_window_peaks(&peaks));
        levels.push(WindowPeakLevel {
            bucket,
            peak: peaks,
        });
        let Some(next) = next else {
            break;
        };
        peaks = next;
        bucket = bucket.saturating_mul(2);
    }
    levels
}

fn combine_window_peaks(previous: &[f32]) -> Vec<f32> {
    let mut peaks = vec![0.0_f32; previous.len().div_ceil(2)];
    for (index, &value) in previous.iter().enumerate() {
        let parent = index / 2;
        if value.abs() > peaks[parent].abs() {
            peaks[parent] = value;
        }
    }
    peaks
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
    let painter = painter.with_clip_rect(rect);
    draw_sample_graph(&painter, rect, waveform, start, end, source_start, gain);
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
    let first = (start.floor().max(0.0) as u64).min(frames - 1);
    let last = (end.ceil().max(0.0) as u64).min(frames - 1).max(first);
    if last <= first {
        return;
    }
    let center = rect.center().y;
    let half = rect.height() * 0.42;
    let mut points = Vec::new();
    let samples_per_pixel = span / f64::from(rect.width().max(1.0));
    if let Some(level) = waveform.window_peak_level_for(samples_per_pixel) {
        let bucket = level.bucket;
        let first_bucket = (first / bucket) as usize;
        let last_bucket = (last / bucket) as usize;
        points.reserve(last_bucket.saturating_sub(first_bucket).saturating_add(1));
        for index in first_bucket..=last_bucket {
            let Some(&value) = level.peak.get(index) else {
                continue;
            };
            let center_frame = index as f64 * bucket as f64 + (bucket as f64 - 1.0) * 0.5;
            let x = rect.left() + ((center_frame - start) / span) as f32 * rect.width();
            let value = graph_value(value, gain, source_start, center_frame);
            points.push(pos2(x, center - value.clamp(-1.0, 1.0) * half));
        }
    } else {
        let stride = samples_per_pixel.round().max(1.0) as u64;
        let mut frame = first;
        loop {
            let bucket_end = frame.saturating_add(stride).min(last.saturating_add(1));
            let mut sum = 0.0_f32;
            let mut count = 0_u64;
            for sample_frame in frame..bucket_end {
                let mut value = waveform.sample(sample_frame);
                if let Some(gain) = gain {
                    value *= gain.gain_at(source_start, sample_frame);
                }
                sum += value;
                count += 1;
            }
            let value = if count == 0 { 0.0 } else { sum / count as f32 };
            let center_frame = (frame as f64 + bucket_end as f64 - 1.0) * 0.5;
            let x = rect.left() + ((center_frame - start) / span) as f32 * rect.width();
            points.push(pos2(x, center - value.clamp(-1.0, 1.0) * half));
            if bucket_end > last {
                break;
            }
            frame = bucket_end;
        }
    }
    crate::widgets::area::fill_polyline_area(painter, &points, center, WAVEFORM);
}

fn graph_value(value: f32, gain: Option<&WaveformGain<'_>>, source_start: u64, frame: f64) -> f32 {
    match gain {
        Some(gain) => value * gain.gain_at_frame(source_start, frame),
        None => value,
    }
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

    pub fn window_peak_level_for(waveform: &AudioWaveform, samples_per_pixel: f64) -> usize {
        waveform
            .window_peak_level_for(samples_per_pixel)
            .map_or(0, |level| level.bucket as usize)
    }

    pub fn window_peak_at(
        waveform: &AudioWaveform,
        samples_per_pixel: f64,
        index: usize,
    ) -> Option<f32> {
        waveform
            .window_peak_level_for(samples_per_pixel)
            .and_then(|level| level.peak.get(index).copied())
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
