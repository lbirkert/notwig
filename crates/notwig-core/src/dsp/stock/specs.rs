use crate::{DeviceKind, ParamId, SchemaVersion};
use uuid::Uuid;

use super::super::devices::{DeviceSpec, DeviceUiSpec, Port, PortDir, SignalType};
use super::super::params::{ParamSpec, Unit};
use super::super::registry::DeviceRegistry;

pub const TOOL_TYPE_ID: &str = "notwig.tool";
pub const EQ_TYPE_ID: &str = "notwig.eq";
pub const CHAIN_TYPE_ID: &str = "notwig.chain";

#[must_use]
pub fn param_id(value: u128) -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(value))
}

pub fn p_input_gain() -> ParamId {
    param_id(0x1001)
}
pub fn p_output_gain() -> ParamId {
    param_id(0x1002)
}
pub fn p_pan() -> ParamId {
    param_id(0x1003)
}
pub fn p_width() -> ParamId {
    param_id(0x1004)
}
pub fn p_polarity_l() -> ParamId {
    param_id(0x1005)
}
pub fn p_polarity_r() -> ParamId {
    param_id(0x1006)
}
pub fn p_dry_wet() -> ParamId {
    param_id(0x1007)
}
pub fn p_swap_lr() -> ParamId {
    param_id(0x1008)
}

pub fn band_param(band: usize, offset: u128) -> ParamId {
    param_id(0x2000 + (band as u128) * 0x10 + offset)
}
pub fn p_eq_output_gain() -> ParamId {
    param_id(0x2100)
}
pub fn p_eq_shift() -> ParamId {
    param_id(0x2101)
}

fn input_gain_param(id: ParamId) -> ParamSpec {
    ParamSpec::continuous(id, "Input Gain", Unit::Decibels, -24.0, 24.0, 0.0)
}

fn output_gain_param(id: ParamId) -> ParamSpec {
    ParamSpec::continuous(id, "Output Gain", Unit::Decibels, -24.0, 24.0, 0.0)
}

fn dry_wet_param(id: ParamId) -> ParamSpec {
    ParamSpec::continuous(id, "Dry/Wet", Unit::Percent, 0.0, 1.0, 1.0)
}

fn audio_ports() -> Vec<Port> {
    vec![
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
    ]
}

#[must_use]
pub fn tool_spec() -> DeviceSpec {
    DeviceSpec {
        type_id: TOOL_TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: audio_ports(),
        params: vec![
            input_gain_param(p_input_gain()),
            output_gain_param(p_output_gain()),
            ParamSpec::continuous(p_pan(), "Pan", Unit::None, -1.0, 1.0, 0.0),
            ParamSpec::continuous(p_width(), "Stereo Width", Unit::Percent, 0.0, 2.0, 1.0),
            ParamSpec::boolean(p_polarity_l(), "Polarity L", false),
            ParamSpec::boolean(p_swap_lr(), "Swap L/R", false),
            ParamSpec::boolean(p_polarity_r(), "Polarity R", false),
            dry_wet_param(p_dry_wet()),
        ],
        latency_samples: 0,
        name: "Tool".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::custom("notwig.tool", Some(300.0)),
        container: None,
    }
}

#[must_use]
pub fn eq_spec() -> DeviceSpec {
    let mut params = Vec::new();
    for band in 0..4 {
        params.push(ParamSpec::boolean(
            band_param(band, 0),
            format!("Band {} Enabled", band + 1),
            false,
        ));
        params.push(ParamSpec::enumeration(
            band_param(band, 1),
            format!("Band {} Type", band + 1),
            5,
            0,
        ));
        let default_freq = [30.0, 200.0, 2_000.0, 8_000.0][band];
        params.push(ParamSpec::continuous(
            band_param(band, 2),
            format!("Band {} Frequency", band + 1),
            Unit::Hertz,
            20.0,
            20_000.0,
            default_freq,
        ));
        params.push(ParamSpec::continuous(
            band_param(band, 3),
            format!("Band {} Gain", band + 1),
            Unit::Decibels,
            -24.0,
            24.0,
            0.0,
        ));
        params.push(ParamSpec::continuous(
            band_param(band, 4),
            format!("Band {} Q", band + 1),
            Unit::None,
            0.1,
            18.0,
            0.707,
        ));
    }
    params.push(output_gain_param(p_eq_output_gain()));
    params.push(ParamSpec::continuous(
        p_eq_shift(),
        "Shift",
        Unit::Semitones,
        -12.0,
        12.0,
        0.0,
    ));
    DeviceSpec {
        type_id: EQ_TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: audio_ports(),
        params,
        latency_samples: 0,
        name: "EQ".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::custom("notwig.eq", Some(560.0)),
        container: None,
    }
}

#[must_use]
pub fn chain_spec() -> DeviceSpec {
    DeviceSpec {
        type_id: CHAIN_TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::Container,
        ports: audio_ports(),
        params: vec![
            input_gain_param(p_input_gain()),
            output_gain_param(p_output_gain()),
            dry_wet_param(p_dry_wet()),
        ],
        latency_samples: 0,
        name: "Chain".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::custom("notwig.chain", Some(150.0)),
        container: None,
    }
}

#[must_use]
pub fn stock_registry() -> DeviceRegistry {
    let mut registry = DeviceRegistry::new();
    registry.register(tool_spec());
    registry.register(eq_spec());
    registry.register(chain_spec());
    registry
}
