//! Synth device spec.

use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, SchemaVersion};

use super::TYPE_ID;
use super::params::{
    DEFAULT_ATTACK_SECONDS, DEFAULT_DECAY_SECONDS, DEFAULT_OSCILLATOR, DEFAULT_RELEASE_SECONDS,
    DEFAULT_SUSTAIN, MAX_ATTACK_SECONDS, MAX_DECAY_SECONDS, MAX_RELEASE_SECONDS,
    MIN_ATTACK_SECONDS, MIN_DECAY_SECONDS, MIN_RELEASE_SECONDS, attack_id, decay_id, oscillator_id,
    release_id, sustain_id,
};

pub(super) fn spec() -> DeviceSpec {
    DeviceSpec {
        type_id: TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::Instrument,
        ports: vec![
            Port {
                id: "notes".into(),
                dir: PortDir::In,
                signal: SignalType::NoteEvent,
                name: "Notes".into(),
                optional: true,
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
            ParamSpec::enumeration(oscillator_id(), "Oscillator", 4, DEFAULT_OSCILLATOR),
            ParamSpec::continuous(
                attack_id(),
                "Attack",
                Unit::Seconds,
                MIN_ATTACK_SECONDS,
                MAX_ATTACK_SECONDS,
                DEFAULT_ATTACK_SECONDS,
            ),
            ParamSpec::continuous(
                decay_id(),
                "Decay",
                Unit::Seconds,
                MIN_DECAY_SECONDS,
                MAX_DECAY_SECONDS,
                DEFAULT_DECAY_SECONDS,
            ),
            ParamSpec::continuous(
                sustain_id(),
                "Sustain",
                Unit::Percent,
                0.0,
                1.0,
                DEFAULT_SUSTAIN,
            ),
            ParamSpec::continuous(
                release_id(),
                "Release",
                Unit::Seconds,
                MIN_RELEASE_SECONDS,
                MAX_RELEASE_SECONDS,
                DEFAULT_RELEASE_SECONDS,
            ),
        ],
        latency_samples: 0,
        name: "Synth".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(260.0)),
        container: None,
    }
}
