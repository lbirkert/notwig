use notwig_core::TrackId;

use super::graph;
use super::{ActivePlan, TrackRuntime};

impl ActivePlan {
    pub(super) fn apply_track_gain_and_pan(&mut self, id: TrackId, frames: usize, apply_pan: bool) {
        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };
        let (gain_l, gain_r) = track.monitor_gain(apply_pan);
        for frame in track.buffer[..frames * 2].chunks_exact_mut(2) {
            frame[0] *= gain_l;
            frame[1] *= gain_r;
        }
    }

    pub(super) fn route_track(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get(&id) else {
            return;
        };
        let target = graph::output_target(&track.plan, self.master);
        let send_count = track.plan.sends.len();
        if let Some(target_id) = target {
            if target_id != id {
                self.mix_main(id, target_id, frames);
            }
        }
        for index in 0..send_count {
            let Some(send) = self
                .tracks
                .get(&id)
                .and_then(|track| track.plan.sends.get(index))
                .cloned()
            else {
                continue;
            };
            if !send.enabled || send.target == id {
                continue;
            }
            self.mix_send(id, &send, frames);
        }
    }

    fn mix_main(&mut self, source: TrackId, target: TrackId, frames: usize) {
        if source == target || !self.tracks.contains_key(&target) {
            return;
        }
        // Pad only this connection by the difference between the destination's
        // input label and the source's output label. Downstream connections pad
        // themselves, so nested groups are not compensated twice.
        let delay_frames = self
            .tracks
            .get(&source)
            .zip(self.tracks.get(&target))
            .map_or(0, |(source, target)| {
                target
                    .plan
                    .input_latency
                    .saturating_sub(source.plan.output_latency)
            });
        {
            let Some(source_track) = self.tracks.get_mut(&source) else {
                return;
            };
            let TrackRuntime {
                buffer, main_delay, ..
            } = source_track;
            let scratch = &mut self.route_scratch[..frames * 2];
            if delay_frames == 0 {
                scratch.copy_from_slice(&buffer[..frames * 2]);
            } else {
                main_delay.process(&buffer[..frames * 2], scratch, frames);
            }
        }
        self.add_scratch_to_track(target, 1.0, frames);
    }

    fn mix_send(&mut self, source: TrackId, send: &notwig_core::dsp::PlanSend, frames: usize) {
        let Some(source_track) = self.tracks.get_mut(&source) else {
            return;
        };
        // Pre-fader sends carry the track input label; post-fader sends carry the
        // track output label. The plan stores the padding to the destination's
        // input label so the send and direct paths stay phase-coherent.
        let TrackRuntime {
            buffer,
            pre_fader,
            send_delays,
            ..
        } = source_track;
        let input = if send.pre_fader {
            &pre_fader[..frames * 2]
        } else {
            &buffer[..frames * 2]
        };
        let scratch = &mut self.route_scratch[..frames * 2];
        if send.latency_samples == 0 {
            scratch.copy_from_slice(input);
        } else if let Some(delay) = send_delays.get_mut(&send.id) {
            delay.process(input, scratch, frames);
        } else {
            scratch.copy_from_slice(input);
        }
        let gain = send.amount_db.to_gain().0;
        self.add_scratch_to_track(send.target, gain, frames);
    }

    fn add_scratch_to_track(&mut self, target: TrackId, gain: f32, frames: usize) {
        let Some(target) = self.tracks.get_mut(&target) else {
            return;
        };
        for (dst, sample) in target.buffer[..frames * 2]
            .iter_mut()
            .zip(self.route_scratch[..frames * 2].iter())
        {
            *dst += *sample * gain;
        }
    }

    pub(super) fn update_meters(&mut self, id: TrackId, frames: usize) {
        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;
        for frame in track.buffer[..frames * 2].chunks_exact(2) {
            peak_l = peak_l.max(frame[0].abs());
            peak_r = peak_r.max(frame[1].abs());
            sum_l += f64::from(frame[0]) * f64::from(frame[0]);
            sum_r += f64::from(frame[1]) * f64::from(frame[1]);
        }
        let frames = frames.max(1) as f64;
        track.peak_l = peak_l;
        track.peak_r = peak_r;
        track.rms_l = (sum_l / frames).sqrt() as f32;
        track.rms_r = (sum_r / frames).sqrt() as f32;
    }
}

pub(super) fn stereo_peaks(samples: &[f32]) -> (f32, f32) {
    let mut peak_l = 0.0_f32;
    let mut peak_r = 0.0_f32;
    for frame in samples.chunks_exact(2) {
        peak_l = peak_l.max(frame[0].abs());
        peak_r = peak_r.max(frame[1].abs());
    }
    (peak_l, peak_r)
}
