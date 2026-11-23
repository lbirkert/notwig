use super::Engine;

impl Engine {
    pub(super) fn apply_metronome(&mut self, metronome: notwig_core::protocol::MetronomeState) {
        self.metronome_enabled = metronome.enabled;
        self.metronome_beats_per_bar = metronome.beats_per_bar.max(1);
        self.metronome_beat_length = metronome.beat_length;
        self.metronome_bar_length = metronome.bar_length.max(metronome.beat_length);
    }

    pub(super) fn mix_metronome(&self, buffer: &mut [f32], start_sample: u64, frames: usize) {
        if !self.metronome_enabled {
            return;
        }
        let Some(plan) = &self.plan else {
            return;
        };
        let rate = f64::from(self.config.sample_rate.max(1));
        let samples_per_quarter = rate * 60.0 / plan.config.tempo_bpm.max(1e-6);
        let beat_length = self.metronome_beat_length.as_f64().max(1e-9);
        let bar_length = self.metronome_bar_length.as_f64().max(beat_length);
        let samples_per_pulse = samples_per_quarter * beat_length;
        let samples_per_bar = samples_per_quarter * bar_length;
        if samples_per_pulse <= 0.0 || samples_per_bar <= 0.0 {
            return;
        }
        let click_len = (0.03 * rate).max(1.0).min(samples_per_pulse);
        for index in 0..frames {
            let absolute = start_sample as f64 + index as f64;
            let pulse_position = absolute % samples_per_pulse;
            if pulse_position >= click_len {
                continue;
            }
            let bar_position = absolute % samples_per_bar;
            let accent = bar_position < click_len;
            let frequency = if accent { 1760.0 } else { 880.0 };
            let envelope = (1.0 - pulse_position / click_len).powi(2);
            let amplitude = if accent { 0.35 } else { 0.22 };
            let seconds = pulse_position / rate;
            let sample = (seconds * frequency * std::f64::consts::TAU).sin() as f32
                * envelope as f32
                * amplitude;
            buffer[index * 2] += sample;
            buffer[index * 2 + 1] += sample;
        }
    }
}
