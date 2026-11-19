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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    i64::try_from(gcd_i128(i128::from(a), i128::from(b))).unwrap_or(1)
}

impl Add for Rational {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let den = i128::from(self.den) * i128::from(rhs.den);
        let num =
            i128::from(self.num) * i128::from(rhs.den) + i128::from(rhs.num) * i128::from(self.den);
        rational_from_i128(num, den)
    }
}

impl Sub for Rational {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let den = i128::from(self.den) * i128::from(rhs.den);
        let num =
            i128::from(self.num) * i128::from(rhs.den) - i128::from(rhs.num) * i128::from(self.den);
        rational_from_i128(num, den)
    }
}

fn rational_from_i128(mut num: i128, mut den: i128) -> Rational {
    if den < 0 {
        num = -num;
        den = -den;
    }
    let gcd = gcd_i128(num, den);
    num /= gcd;
    den /= gcd;
    let num = i64::try_from(num).unwrap_or(if num.is_negative() {
        i64::MIN
    } else {
        i64::MAX
    });
    let den = u32::try_from(den).unwrap_or(u32::MAX).max(1);
    Rational::new(num, den)
}

fn gcd_i128(a: i128, b: i128) -> i128 {
    let mut a = a.unsigned_abs();
    let mut b = b.unsigned_abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    if a == 0 {
        1
    } else {
        i128::try_from(a).unwrap_or(i128::MAX)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (i128::from(self.num) * i128::from(other.den))
            .cmp(&(i128::from(other.num) * i128::from(self.den)))
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
        GainLinear(self.to_gain_f64() as f32)
    }

    #[must_use]
    pub fn to_gain_f64(self) -> f64 {
        10.0_f64.powf(f64::from(self.0) / 20.0)
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
