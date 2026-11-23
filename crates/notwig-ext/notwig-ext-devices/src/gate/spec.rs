//! Gate device spec.

use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, SchemaVersion};

use super::TYPE_ID;
use super::params::{
    DEFAULT_ATTACK_SECONDS, DEFAULT_HOLD_SECONDS, DEFAULT_RANGE_DB, DEFAULT_RELEASE_SECONDS,
    DEFAULT_THRESHOLD_DB, MAX_ATTACK_SECONDS, MAX_HOLD_SECONDS, MAX_RANGE_DB, MAX_RELEASE_SECONDS,
    MAX_THRESHOLD_DB, MIN_ATTACK_SECONDS, MIN_HOLD_SECONDS, MIN_RANGE_DB, MIN_RELEASE_SECONDS,
    MIN_THRESHOLD_DB, attack_id, hold_id, range_id, release_id, threshold_id,
};

pub(super) fn spec() -> DeviceSpec {
    DeviceSpec {
        type_id: TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: vec![
            Port {
                id: "in".into(),
                dir: PortDir::In,
                signal: SignalType::Audio(2),
                name: "Input".into(),
                optional: false,
            },
            Port {
                id: "out".into(),
                dir: PortDir::Out,
                signal: SignalType::Audio(2),
                name: "Output".into(),
                optional: false,
            },
        ],
        params: vec![
            ParamSpec::continuous(
                threshold_id(),
                "Threshold",
                Unit::Decibels,
                MIN_THRESHOLD_DB,
                MAX_THRESHOLD_DB,
                DEFAULT_THRESHOLD_DB,
            ),
            ParamSpec::continuous(
                attack_id(),
                "Attack",
                Unit::Seconds,
                MIN_ATTACK_SECONDS,
                MAX_ATTACK_SECONDS,
                DEFAULT_ATTACK_SECONDS,
            ),
            ParamSpec::continuous(
                hold_id(),
                "Hold",
                Unit::Seconds,
                MIN_HOLD_SECONDS,
                MAX_HOLD_SECONDS,
                DEFAULT_HOLD_SECONDS,
            ),
            ParamSpec::continuous(
                release_id(),
                "Release",
                Unit::Seconds,
                MIN_RELEASE_SECONDS,
                MAX_RELEASE_SECONDS,
                DEFAULT_RELEASE_SECONDS,
            ),
            ParamSpec::continuous(
                range_id(),
                "Range",
                Unit::Decibels,
                MIN_RANGE_DB,
                MAX_RANGE_DB,
                DEFAULT_RANGE_DB,
            ),
        ],
        latency_samples: 0,
        name: "Gate".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(260.0)),
        container: None,
    }
}
