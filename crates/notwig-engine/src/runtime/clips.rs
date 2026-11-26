use notwig_core::{AudioAssetBuffer, ClipPayload, Curve, Db, TempoMap};

use super::{ActivePlan, TrackRuntime};

impl ActivePlan {
    pub(super) fn render_audio_clips(&mut self, start_sample: u64, frames: usize) {
        let map = notwig_core::TempoMap {
            tempo_bpm: self.tempo_bpm,
            sample_rate: notwig_core::SampleRate(self.sample_rate),
        };
        for (track_id, clips) in &self.schedule.tracks {
            let Some(track) = self.tracks.get_mut(track_id) else {
                continue;
            };
            if !track.plan.active {
                continue;
            }
            for clip in clips {
                let ClipPayload::Audio(audio) = &clip.payload else {
                    continue;
                };
                let clip_start = map.mus_to_samples(clip.range.start);
                let clip_end = map.mus_to_samples(clip.range.end);
                if clip_end <= start_sample || clip_start >= start_sample + frames as u64 {
                    continue;
                }
                if audio.uses_slices() {
                    let clip_start = i64::try_from(clip_start).unwrap_or(i64::MAX);
                    let clip_end = i64::try_from(clip_end).unwrap_or(i64::MAX);
                    for slice in &audio.slices {
                        let Some(asset) =
                            self.assets.get(&slice.asset_id.unwrap_or(audio.asset_id))
                        else {
                            continue;
                        };
                        if asset.channels == 0 {
                            continue;
                        }
                        let source_len =
                            i64::try_from(slice.duration_samples()).unwrap_or(i64::MAX);
                        if source_len <= 0 {
                            continue;
                        }
                        let slice_start = clip_start + map.samples_delta(slice.start);
                        let slice_end = slice_start.saturating_add(source_len);
                        let visible_start = slice_start.max(clip_start).max(0);
                        let visible_end = slice_end.min(clip_end);
                        if visible_end <= visible_start {
                            continue;
                        }
                        let offset = u64::try_from(visible_start - slice_start).unwrap_or(0);
                        let audio_offset = offset.max(slice.silent_prefix_samples);
                        let region_start = slice_start
                            .saturating_add(i64::try_from(audio_offset).unwrap_or(i64::MAX));
                        if region_start >= visible_end
                            || slice.source_start_sample >= slice.source_end_sample
                        {
                            continue;
                        }
                        let source_start = slice
                            .source_start_sample
                            .saturating_add(audio_offset - slice.silent_prefix_samples);
                        if source_start >= slice.source_end_sample {
                            continue;
                        }
                        render_audio_region(
                            track,
                            asset,
                            u64::try_from(region_start).unwrap_or(0),
                            u64::try_from(visible_end).unwrap_or(0),
                            source_start,
                            slice.source_end_sample,
                            slice.gain_db.to_gain().0,
                            audio.gain_curve.as_ref(),
                            u64::try_from(clip_start).unwrap_or(0),
                            &map,
                            0,
                            0,
                            start_sample,
                            frames,
                        );
                    }
                } else {
                    let Some(asset) = self.assets.get(&audio.asset_id) else {
                        continue;
                    };
                    if asset.channels == 0 {
                        continue;
                    }
                    render_audio_region(
                        track,
                        asset,
                        clip_start,
                        clip_end,
                        audio.source_start_sample,
                        audio.source_end_sample,
                        audio.gain_db.to_gain().0,
                        audio.gain_curve.as_ref(),
                        clip_start,
                        &map,
                        audio.fade_in_samples,
                        audio.fade_out_samples,
                        start_sample,
                        frames,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_audio_region(
    track: &mut TrackRuntime,
    asset: &AudioAssetBuffer,
    region_start: u64,
    region_end: u64,
    source_start: u64,
    source_end: u64,
    gain: f32,
    gain_curve: Option<&Curve>,
    clip_start_sample: u64,
    map: &TempoMap,
    fade_in_samples: u64,
    fade_out_samples: u64,
    start_sample: u64,
    frames: usize,
) {
    for i in 0..frames {
        let absolute = start_sample + i as u64;
        if absolute < region_start || absolute >= region_end {
            continue;
        }
        let source = source_start + (absolute - region_start);
        if source >= source_end {
            continue;
        }
        let fade_in = if fade_in_samples == 0 {
            1.0
        } else {
            ((source - source_start) as f32 / fade_in_samples as f32).min(1.0)
        };
        let fade_out = if fade_out_samples == 0 {
            1.0
        } else {
            ((source_end - source) as f32 / fade_out_samples as f32).min(1.0)
        };
        let curve_gain = gain_curve.map_or(1.0, |curve| {
            let local = map.samples_to_mus(absolute.saturating_sub(clip_start_sample));
            Db(curve.value_at(local, 0.0) as f32).to_gain().0
        });
        let envelope = fade_in.min(fade_out) * gain * curve_gain;
        let (left, right) = sample_frame(asset, source);
        let idx = i * 2;
        track.buffer[idx] += left * envelope;
        track.buffer[idx + 1] += right * envelope;
    }
}

fn sample_frame(asset: &AudioAssetBuffer, frame: u64) -> (f32, f32) {
    let channels = usize::from(asset.channels.max(1));
    let start = frame as usize * channels;
    match channels {
        1 => {
            let value = asset.samples.get(start).copied().unwrap_or(0.0);
            (value, value)
        }
        _ => (
            asset.samples.get(start).copied().unwrap_or(0.0),
            asset.samples.get(start + 1).copied().unwrap_or(0.0),
        ),
    }
}
