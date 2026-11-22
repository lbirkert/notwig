#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SmoothedValue {
    current: f64,
    target: f64,
    coefficient: f64,
}

impl SmoothedValue {
    pub(crate) fn new(value: f64, sample_rate: f64, milliseconds: f64) -> Self {
        let mut smoothed = Self {
            current: value,
            target: value,
            coefficient: 1.0,
        };
        smoothed.set_sample_rate(sample_rate, milliseconds);
        smoothed
    }

    pub(crate) fn set_sample_rate(&mut self, sample_rate: f64, milliseconds: f64) {
        let samples = (sample_rate.max(1.0) * milliseconds.max(0.1) / 1_000.0).max(1.0);
        self.coefficient = 1.0 - (-1.0 / samples).exp();
    }

    pub(crate) fn set_target(&mut self, value: f64) {
        self.target = value;
    }

    pub(crate) fn snap(&mut self) {
        self.current = self.target;
    }

    #[inline]
    pub(crate) fn next(&mut self) -> f64 {
        self.current += (self.target - self.current) * self.coefficient;
        if (self.target - self.current).abs() < 1.0e-9 {
            self.current = self.target;
        }
        self.current
    }
}
