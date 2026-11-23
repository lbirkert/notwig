//! Synth parameter ids, ranges, defaults, and normalization.

use notwig_core::ParamId;
use uuid::Uuid;

const OSCILLATOR_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0001;
const ATTACK_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0002;
const DECAY_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0003;
const SUSTAIN_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0004;
const RELEASE_UUID: u128 = 0x5359_4e54_4800_0000_0000_0000_0000_0005;

pub(super) const DEFAULT_OSCILLATOR: u32 = 0;
pub(super) const DEFAULT_ATTACK_SECONDS: f64 = 0.01;
pub(super) const DEFAULT_DECAY_SECONDS: f64 = 0.2;
pub(super) const DEFAULT_SUSTAIN: f64 = 0.7;
pub(super) const DEFAULT_RELEASE_SECONDS: f64 = 0.2;

pub(super) const MIN_ATTACK_SECONDS: f64 = 0.001;
pub(super) const MAX_ATTACK_SECONDS: f64 = 2.0;
pub(super) const MIN_DECAY_SECONDS: f64 = 0.001;
pub(super) const MAX_DECAY_SECONDS: f64 = 2.0;
pub(super) const MIN_RELEASE_SECONDS: f64 = 0.001;
pub(super) const MAX_RELEASE_SECONDS: f64 = 4.0;

pub(super) const OSCILLATOR_OPTIONS: [&str; 4] = ["Sine", "Square", "Saw", "Triangle"];

/// Returns the oscillator waveform parameter id.
#[must_use]
pub fn oscillator_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(OSCILLATOR_UUID))
}

/// Returns the synth attack parameter id.
#[must_use]
pub fn attack_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_UUID))
}

/// Returns the synth decay parameter id.
#[must_use]
pub fn decay_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DECAY_UUID))
}

/// Returns the synth sustain parameter id.
#[must_use]
pub fn sustain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(SUSTAIN_UUID))
}

/// Returns the synth release parameter id.
#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

pub(super) fn attack_norm(seconds: f64) -> f32 {
    ((seconds - MIN_ATTACK_SECONDS) / (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)) as f32
}

pub(super) fn decay_norm(seconds: f64) -> f32 {
    ((seconds - MIN_DECAY_SECONDS) / (MAX_DECAY_SECONDS - MIN_DECAY_SECONDS)) as f32
}

pub(super) fn release_norm(seconds: f64) -> f32 {
    ((seconds - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

pub(super) fn attack_seconds(norm: f32) -> f64 {
    MIN_ATTACK_SECONDS + f64::from(norm) * (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)
}

pub(super) fn decay_seconds(norm: f32) -> f64 {
    MIN_DECAY_SECONDS + f64::from(norm) * (MAX_DECAY_SECONDS - MIN_DECAY_SECONDS)
}

pub(super) fn release_seconds(norm: f32) -> f64 {
    MIN_RELEASE_SECONDS + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)
}
