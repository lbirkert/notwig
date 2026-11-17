use std::collections::BTreeMap;

use egui::{Color32, Painter, Rect, Stroke};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{AssetId, ClipEntity, ProjectDoc};

pub(crate) fn draw_waveform(
    painter: &Painter,
    rect: Rect,
    peaks: &[f32],
    asset_samples: u64,
    source_start: u64,
    source_end: u64,
) {
    if peaks.is_empty() || source_end <= source_start || rect.width() < 2.0 {
        return;
    }
    let source_start = source_start.min(asset_samples) as f64;
    let source_end = source_end.min(asset_samples) as f64;
    if source_end <= source_start {
        return;
    }
    let center = rect.center().y;
    let half = rect.height() * 0.34;
    let steps = (rect.width() / 2.0).max(1.0) as usize;
    for step in 0..steps {
        let t = step as f64 / steps as f64;
        let source = source_start + t * (source_end - source_start);
        let index = peak_index(source, asset_samples, peaks.len());
        let peak = peaks.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let x = rect.left() + t as f32 * rect.width();
        painter.line_segment(
            [
                egui::pos2(x, center - peak * half),
                egui::pos2(x, center + peak * half),
            ],
            Stroke::new(1.0_f32, Color32::from_black_alpha(120)),
        );
    }
}

fn peak_index(source_sample: f64, asset_samples: u64, peaks: usize) -> usize {
    if asset_samples == 0 || peaks == 0 {
        return 0;
    }
    ((source_sample / asset_samples as f64) * peaks as f64) as usize
}

pub(crate) fn audio_slices(clip: &ClipEntity, audio: &AudioClipData) -> Vec<AudioSlice> {
    if audio.uses_slices() {
        return audio.slices.clone();
    }
    vec![AudioSlice {
        id: notwig_core::SliceId::from_uuid(*clip.id.as_uuid()),
        start: notwig_core::TimeMus::zero(),
        source_start_sample: audio.source_start_sample,
        source_end_sample: audio.source_end_sample,
        gain_db: audio.gain_db,
        asset_id: None,
    }]
}

pub(crate) fn draw_audio_slices(
    painter: &Painter,
    doc: &ProjectDoc,
    clip: &ClipEntity,
    audio: &AudioClipData,
    waveforms: &BTreeMap<AssetId, Vec<f32>>,
    body_rect: Rect,
    grid: crate::timeline::TimelineGrid,
    lane_left: f32,
    lane_top: f32,
) {
    let asset_samples = doc
        .assets
        .assets
        .get(&audio.asset_id)
        .and_then(|asset| asset.duration_samples)
        .unwrap_or(audio.source_end_sample);
    if !audio.uses_slices() {
        if let Some(peaks) = waveforms.get(&audio.asset_id) {
            draw_waveform(
                painter,
                body_rect,
                peaks,
                asset_samples,
                audio.source_start_sample,
                audio.source_end_sample,
            );
        }
        return;
    }
    let map = doc.tempo_map();
    for slice in &audio.slices {
        let start = clip.range.start + slice.start;
        let duration = map.samples_to_mus(
            slice
                .source_end_sample
                .saturating_sub(slice.source_start_sample),
        );
        let x1 = grid.x_for_beat(start, lane_left);
        let x2 = grid.x_for_beat(start + duration, lane_left);
        let rect = Rect::from_min_max(
            egui::pos2(x1, body_rect.top()),
            egui::pos2(x2, body_rect.bottom()),
        )
        .intersect(Rect::from_min_max(
            egui::pos2(body_rect.left(), lane_top),
            body_rect.max,
        ));
        let asset_id = slice.asset_id.unwrap_or(audio.asset_id);
        let Some(peaks) = waveforms.get(&asset_id) else {
            continue;
        };
        let asset_samples = doc
            .assets
            .assets
            .get(&asset_id)
            .and_then(|asset| asset.duration_samples)
            .unwrap_or(slice.source_end_sample);
        if rect.width() > 1.0 {
            draw_waveform(
                painter,
                rect,
                peaks,
                asset_samples,
                slice.source_start_sample,
                slice.source_end_sample,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_source_samples_map_to_asset_peaks_without_stretching() {
        assert_eq!(peak_index(0.0, 1_000, 10), 0);
        assert_eq!(peak_index(500.0, 1_000, 10), 5);
        assert_eq!(peak_index(750.0, 1_000, 10), 7);
        assert_eq!(peak_index(999.0, 1_000, 10), 9);
    }
}
