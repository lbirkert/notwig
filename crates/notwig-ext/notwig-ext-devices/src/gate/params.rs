//! Gate parameter ids, ranges, defaults, and normalization.

use notwig_core::ParamId;
use uuid::Uuid;

const THRESHOLD_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0001;
const ATTACK_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0002;
const HOLD_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0003;
const RELEASE_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0004;
const RANGE_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0005;

pub(super) const DEFAULT_THRESHOLD_DB: f64 = -40.0;
pub(super) const DEFAULT_ATTACK_SECONDS: f64 = 0.005;
pub(super) const DEFAULT_HOLD_SECONDS: f64 = 0.05;
pub(super) const DEFAULT_RELEASE_SECONDS: f64 = 0.1;
pub(super) const DEFAULT_RANGE_DB: f64 = -80.0;

pub(super) const MIN_THRESHOLD_DB: f64 = -80.0;
pub(super) const MAX_THRESHOLD_DB: f64 = 0.0;
pub(super) const MIN_ATTACK_SECONDS: f64 = 0.001;
pub(super) const MAX_ATTACK_SECONDS: f64 = 0.1;
pub(super) const MIN_HOLD_SECONDS: f64 = 0.0;
pub(super) const MAX_HOLD_SECONDS: f64 = 0.5;
pub(super) const MIN_RELEASE_SECONDS: f64 = 0.01;
pub(super) const MAX_RELEASE_SECONDS: f64 = 1.0;
pub(super) const MIN_RANGE_DB: f64 = -80.0;
pub(super) const MAX_RANGE_DB: f64 = 0.0;

/// Returns the gate threshold parameter id.
#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

/// Returns the gate attack parameter id.
#[must_use]
pub fn attack_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_UUID))
}

/// Returns the gate hold parameter id.
#[must_use]
pub fn hold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(HOLD_UUID))
}

/// Returns the gate release parameter id.
#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

/// Returns the gate range parameter id.
#[must_use]
pub fn range_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RANGE_UUID))
}

pub(super) fn threshold_norm(value_db: f64) -> f32 {
    ((value_db - MIN_THRESHOLD_DB) / (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)) as f32
}

pub(super) fn attack_norm(seconds: f64) -> f32 {
    ((seconds - MIN_ATTACK_SECONDS) / (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)) as f32
}

pub(super) fn hold_norm(seconds: f64) -> f32 {
    ((seconds - MIN_HOLD_SECONDS) / (MAX_HOLD_SECONDS - MIN_HOLD_SECONDS)) as f32
}

pub(super) fn release_norm(seconds: f64) -> f32 {
    ((seconds - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

pub(super) fn range_norm(value_db: f64) -> f32 {
    ((value_db - MIN_RANGE_DB) / (MAX_RANGE_DB - MIN_RANGE_DB)) as f32
}

pub(super) fn threshold_db(norm: f32) -> f64 {
    MIN_THRESHOLD_DB + f64::from(norm) * (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)
}

pub(super) fn attack_seconds(norm: f32) -> f64 {
    MIN_ATTACK_SECONDS + f64::from(norm) * (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)
}

pub(super) fn hold_seconds(norm: f32) -> f64 {
    MIN_HOLD_SECONDS + f64::from(norm) * (MAX_HOLD_SECONDS - MIN_HOLD_SECONDS)
}

pub(super) fn release_seconds(norm: f32) -> f64 {
    MIN_RELEASE_SECONDS + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)
}

pub(super) fn range_db(norm: f32) -> f64 {
    MIN_RANGE_DB + f64::from(norm) * (MAX_RANGE_DB - MIN_RANGE_DB)
}
