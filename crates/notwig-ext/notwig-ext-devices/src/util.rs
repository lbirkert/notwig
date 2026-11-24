//! Shared DSP helpers for the stock extension devices.

/// Converts dBFS to linear gain.
#[must_use]
pub(crate) fn db_to_gain(db: f64) -> f32 {
    10.0_f64.powf(db / 20.0) as f32
}

/// Converts linear gain to dBFS with a floor.
#[must_use]
pub(crate) fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.max(1.0e-6).log10()
}

/// Flushes values that would otherwise decay into denormals.
#[inline]
pub(crate) fn flush_denormal(value: f32) -> f32 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

/// One-pole smoothing coefficient for a time constant in seconds.
#[must_use]
pub(crate) fn time_coeff(seconds: f64, sample_rate: f64) -> f32 {
    let samples = (seconds * sample_rate).max(1.0);
    (1.0 - (-1.0 / samples).exp()) as f32
}

/// Converts seconds to a whole number of samples.
#[must_use]
pub(crate) fn seconds_to_samples(seconds: f64, sample_rate: f64) -> usize {
    (seconds * sample_rate).round().max(0.0) as usize
}

/// First-order low-pass filter.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    coefficient: f32,
    state: f32,
}

impl OnePole {
    /// Creates a low-pass with a time constant in seconds.
    #[must_use]
    pub(crate) fn new(seconds: f64, sample_rate: f64) -> Self {
        Self {
            coefficient: time_coeff(seconds, sample_rate).clamp(0.0, 1.0),
            state: 0.0,
        }
    }

    /// Updates the time constant.
    pub(crate) fn set_time(&mut self, seconds: f64, sample_rate: f64) {
        self.coefficient = time_coeff(seconds, sample_rate).clamp(0.0, 1.0);
    }

    /// Processes one sample.
    #[inline]
    pub(crate) fn process(&mut self, input: f32) -> f32 {
        self.state += (input - self.state) * self.coefficient;
        self.state = flush_denormal(self.state);
        self.state
    }
}

/// Parameter smoother with a fixed time constant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Smoothed {
    current: f32,
    target: f32,
    coefficient: f32,
}

impl Smoothed {
    /// Creates a smoother initialized to `value`.
    #[must_use]
    pub(crate) fn new(value: f32, milliseconds: f64, sample_rate: f64) -> Self {
        let mut smoothed = Self {
            current: value,
            target: value,
            coefficient: 1.0,
        };
        smoothed.set_sample_rate(sample_rate, milliseconds);
        smoothed
    }

    /// Updates the sample rate and time constant.
    pub(crate) fn set_sample_rate(&mut self, sample_rate: f64, milliseconds: f64) {
        self.coefficient = time_coeff(milliseconds / 1_000.0, sample_rate).clamp(0.0, 1.0);
    }

    /// Sets the target value.
    pub(crate) fn set_target(&mut self, value: f32) {
        self.target = value;
    }

    /// Jumps immediately to the target.
    pub(crate) fn snap(&mut self) {
        self.current = self.target;
    }

    /// Advances by one sample.
    #[inline]
    pub(crate) fn next(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coefficient;
        if (self.target - self.current).abs() < 1.0e-7 {
            self.current = self.target;
        }
        self.current
    }
}

/// Stereo-friendly circular delay line with linear fractional reads.
#[derive(Debug, Clone, Default)]
pub(crate) struct DelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl DelayLine {
    /// Creates an empty line.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Resizes the line to hold at least `samples` samples.
    pub(crate) fn resize(&mut self, samples: usize) {
        if self.buffer.len() != samples.max(1) {
            self.buffer = vec![0.0; samples.max(1)];
            self.write = 0;
        }
    }

    /// Clears the buffer and resets the write position.
    pub(crate) fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write = 0;
    }

    /// Writes one sample and advances.
    #[inline]
    pub(crate) fn push(&mut self, sample: f32) {
        if self.buffer.is_empty() {
            return;
        }
        self.buffer[self.write] = sample;
        self.write += 1;
        if self.write >= self.buffer.len() {
            self.write = 0;
        }
    }

    /// Reads `delay` samples back from the most recent write.
    #[inline]
    pub(crate) fn read(&self, delay: f32) -> f32 {
        let len = self.buffer.len();
        if len == 0 {
            return 0.0;
        }
        let max_delay = (len - 1) as f32;
        let delay = delay.clamp(0.0, max_delay);
        let position = self.write as f32 - delay;
        let position = position.rem_euclid(len as f32);
        let index = position.floor() as usize;
        let fraction = position - index as f32;
        let next = (index + 1) % len;
        self.buffer[index] + (self.buffer[next] - self.buffer[index]) * fraction
    }
}

/// A soft-clip curve blended between hard clipping and `tanh`.
#[must_use]
pub(crate) fn soft_clip(input: f32, softness: f32) -> f32 {
    let hard = input.clamp(-1.0, 1.0);
    let soft = input.tanh();
    hard + (soft - hard) * softness.clamp(0.0, 1.0)
}
