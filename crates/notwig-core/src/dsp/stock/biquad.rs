#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EqBandType {
    Bell,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

impl EqBandType {
    pub(super) fn from_index(index: u32) -> Self {
        match index {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            3 => Self::HighPass,
            4 => Self::LowPass,
            _ => Self::Bell,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Biquad {
    pub(super) b0: f64,
    pub(super) b1: f64,
    pub(super) b2: f64,
    pub(super) a1: f64,
    pub(super) a2: f64,
}

impl Biquad {
    pub(super) fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    pub(super) fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            b0: self.b0 + (other.b0 - self.b0) * t,
            b1: self.b1 + (other.b1 - self.b1) * t,
            b2: self.b2 + (other.b2 - self.b2) * t,
            a1: self.a1 + (other.a1 - self.a1) * t,
            a2: self.a2 + (other.a2 - self.a2) * t,
        }
    }

    pub(super) fn set(
        &mut self,
        kind: EqBandType,
        freq: f64,
        gain_db: f64,
        q: f64,
        sample_rate: f64,
    ) {
        let freq = freq.clamp(10.0, sample_rate * 0.49);
        let w0 = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let q = q.max(0.001);
        let alpha = sin_w0 / (2.0 * q);
        let a = 10.0_f64.powf(gain_db / 40.0);

        // RBJ audio-EQ cookbook coefficients. `a` is the linear gain and the
        // shelves share the sqrt(A) alpha term. Coefficients are normalized by
        // a0 on store, keeping the denominator in the `1 + a1 z^-1 + a2 z^-2`
        // convention that `process` expects.
        let (b0, b1, b2, a0, a1, a2) = match kind {
            EqBandType::Bell => (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            ),
            EqBandType::LowShelf => {
                let sqrt_a = a.sqrt();
                let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            EqBandType::HighShelf => {
                let sqrt_a = a.sqrt();
                let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            EqBandType::HighPass => (
                f64::midpoint(1.0, cos_w0),
                -(1.0 + cos_w0),
                f64::midpoint(1.0, cos_w0),
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            EqBandType::LowPass => (
                f64::midpoint(1.0, -cos_w0),
                1.0 - cos_w0,
                f64::midpoint(1.0, -cos_w0),
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline]
    pub(super) fn process(&self, x: f64, state: &mut [f64; 2]) -> f64 {
        // Direct-form-II-transposed: state[0]/state[1] are the two delay
        // accumulators (z1, z2), so only two values per channel are stored.
        // a1/a2 enter negated here because the denominator uses the
        // `1 + a1 z^-1 + a2 z^-2` convention.
        let y = self.b0 * x + state[0];
        state[0] = self.b1 * x - self.a1 * y + state[1];
        state[1] = self.b2 * x - self.a2 * y;
        y
    }
}
