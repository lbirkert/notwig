use std::fmt;
use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
}

impl SchemaVersion {
    pub const V0: Self = Self { major: 0, minor: 1 };

    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawRational", into = "RawRational")]
pub struct Rational {
    num: i64,
    den: u32,
}

#[derive(Serialize, Deserialize)]
struct RawRational {
    num: i64,
    den: u32,
}

impl TryFrom<RawRational> for Rational {
    type Error = String;

    fn try_from(raw: RawRational) -> Result<Self, Self::Error> {
        Rational::try_new(raw.num, raw.den).map_err(|err| err.to_string())
    }
}

impl From<Rational> for RawRational {
    fn from(value: Rational) -> Self {
        Self {
            num: value.num,
            den: value.den,
        }
    }
}

impl Rational {
    pub fn try_new(num: i64, den: u32) -> Result<Self, CoreError> {
        if den == 0 {
            return Err(CoreError::ValidationFailed {
                reason: "rational denominator must be positive".into(),
            });
        }
        let gcd = gcd_i64(num, i64::from(den));
        Ok(Self {
            num: num / gcd,
            den: den / u32::try_from(gcd.abs()).unwrap_or(1),
        })
    }

    #[must_use]
    pub fn new(num: i64, den: u32) -> Self {
        Self::try_new(num, den).expect("valid rational denominator")
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    #[must_use]
    pub const fn num(self) -> i64 {
        self.num
    }

    #[must_use]
    pub const fn den(self) -> u32 {
        self.den
    }

    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.num as f64 / f64::from(self.den)
    }

    #[must_use]
    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    #[must_use]
    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }

    #[must_use]
    pub fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let mut a = a.abs();
    let mut b = b.abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    if a == 0 { 1 } else { a }
}

impl Add for Rational {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let den = self.den * rhs.den;
        let num = self.num * i64::from(rhs.den) + rhs.num * i64::from(self.den);
        Self::new(num, den)
    }
}

impl Sub for Rational {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let den = self.den * rhs.den;
        let num = self.num * i64::from(rhs.den) - rhs.num * i64::from(self.den);
        Self::new(num, den)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

pub type TimeMus = Rational;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeAudio(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeMus {
    pub start: TimeMus,
    pub end: TimeMus,
}

impl RangeMus {
    pub fn new(start: TimeMus, end: TimeMus) -> Result<Self, CoreError> {
        if end < start {
            return Err(CoreError::ValidationFailed {
                reason: "range end must not precede start".into(),
            });
        }
        Ok(Self { start, end })
    }

    pub fn non_empty(start: TimeMus, end: TimeMus) -> Result<Self, CoreError> {
        let range = Self::new(start, end)?;
        if range.is_empty() {
            return Err(CoreError::ValidationFailed {
                reason: "range must be non-empty".into(),
            });
        }
        Ok(range)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub fn length(self) -> TimeMus {
        self.end - self.start
    }

    #[must_use]
    pub fn contains(self, t: TimeMus) -> bool {
        self.start <= t && t < self.end
    }

    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SampleRate(pub u32);

impl SampleRate {
    pub const DEFAULT: Self = Self(48_000);

    pub fn new(value: u32) -> Result<Self, CoreError> {
        if value == 0 {
            return Err(CoreError::ValidationFailed {
                reason: "sample rate must be positive".into(),
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Db(pub f32);

impl Db {
    pub const MIN: Self = Self(-96.0);

    #[must_use]
    pub fn to_gain(self) -> GainLinear {
        GainLinear(10.0_f32.powf(self.0 / 20.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GainLinear(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pan(pub f32);

impl Pan {
    #[must_use]
    pub fn clamped(value: f32) -> Self {
        Self(value.clamp(-1.0, 1.0))
    }

    #[must_use]
    pub fn gains(self) -> (f32, f32) {
        let angle = (self.0 + 1.0) * 0.25 * std::f32::consts::PI;
        (angle.cos(), angle.sin())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Norm01(pub f32);

impl Norm01 {
    #[must_use]
    pub fn clamped(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hz(pub f64);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Semitones(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMap {
    pub tempo_bpm: f64,
    pub sample_rate: SampleRate,
}

impl TempoMap {
    pub fn samples_per_beat(self) -> f64 {
        f64::from(self.sample_rate.value()) * 60.0 / self.tempo_bpm
    }

    #[must_use]
    pub fn mus_to_samples(self, time: TimeMus) -> u64 {
        let samples = time.as_f64() * self.samples_per_beat();
        if samples <= 0.0 {
            0
        } else {
            samples.round() as u64
        }
    }

    #[must_use]
    pub fn samples_to_mus(self, samples: u64) -> TimeMus {
        let beats = samples as f64 / self.samples_per_beat();
        Rational::new((beats * 1_000_000.0).round() as i64, 1_000_000)
    }

    pub fn samples_delta(self, delta: TimeMus) -> i64 {
        (delta.as_f64() * self.samples_per_beat()).round() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_normalizes() {
        assert_eq!(Rational::new(2, 4), Rational::new(1, 2));
        assert_eq!(Rational::new(-2, 4), Rational::new(-1, 2));
    }

    #[test]
    fn tempo_map_roundtrip_is_near_exact() {
        let map = TempoMap {
            tempo_bpm: 120.0,
            sample_rate: SampleRate::DEFAULT,
        };
        assert_eq!(map.mus_to_samples(Rational::new(1, 1)), 24_000);
        assert_eq!(map.samples_to_mus(24_000), Rational::new(1, 1));
    }

    #[test]
    fn constant_power_pan_is_minus_three_db_center() {
        let (l, r) = Pan(0.0).gains();
        let channel_db = 20.0 * l.log10();
        assert!((channel_db + 3.0103).abs() < 0.01);
        assert!((l - r).abs() < 1e-6);
    }
}
