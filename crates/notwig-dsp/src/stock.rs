use std::collections::BTreeMap;

use notwig_core::{DeviceInstance, DeviceKind, ParamId, ParamValue, SchemaVersion};
use uuid::Uuid;

use crate::devices::{
    DeviceProcessor, DeviceSpec, Latency, Port, PortDir, ProcessContext, ProcessStatus, SignalType,
};
use crate::graph::GraphError;
use crate::params::{ParamSpec, Unit};
use crate::registry::DeviceRegistry;

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

pub fn band_param(band: usize, offset: u128) -> ParamId {
    param_id(0x2000 + (band as u128) * 0x10 + offset)
}
pub fn p_eq_output_gain() -> ParamId {
    param_id(0x2100)
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
            ParamSpec::continuous(
                p_input_gain(),
                "Input Gain",
                Unit::Decibels,
                -24.0,
                24.0,
                0.0,
            ),
            ParamSpec::continuous(
                p_output_gain(),
                "Output Gain",
                Unit::Decibels,
                -24.0,
                24.0,
                0.0,
            ),
            ParamSpec::continuous(p_pan(), "Pan", Unit::None, -1.0, 1.0, 0.0),
            ParamSpec::continuous(p_width(), "Stereo Width", Unit::Percent, 0.0, 2.0, 1.0),
            ParamSpec::boolean(p_polarity_l(), "Polarity L", false),
            ParamSpec::boolean(p_polarity_r(), "Polarity R", false),
            ParamSpec::continuous(p_dry_wet(), "Dry/Wet", Unit::Percent, 0.0, 1.0, 1.0),
        ],
        latency_samples: 0,
        name: "Tool".into(),
        vendor: "Notwig".into(),
    }
}

#[must_use]
pub fn eq_spec() -> DeviceSpec {
    let mut params = Vec::new();
    for band in 0..4 {
        params.push(ParamSpec::boolean(
            band_param(band, 0),
            format!("Band {} Enabled", band + 1),
            true,
        ));
        params.push(ParamSpec::enumeration(
            band_param(band, 1),
            format!("Band {} Type", band + 1),
            5,
            match band {
                0 => 3,
                3 => 2,
                _ => 0,
            },
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
            if band == 0 { 0.0 } else { 0.0 },
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
    params.push(ParamSpec::continuous(
        p_eq_output_gain(),
        "Output Gain",
        Unit::Decibels,
        -24.0,
        24.0,
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
            ParamSpec::continuous(
                p_input_gain(),
                "Input Gain",
                Unit::Decibels,
                -24.0,
                24.0,
                0.0,
            ),
            ParamSpec::continuous(
                p_output_gain(),
                "Output Gain",
                Unit::Decibels,
                -24.0,
                24.0,
                0.0,
            ),
            ParamSpec::continuous(p_dry_wet(), "Dry/Wet", Unit::Percent, 0.0, 1.0, 1.0),
        ],
        latency_samples: 0,
        name: "Chain".into(),
        vendor: "Notwig".into(),
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

#[must_use]
pub fn instantiate_stock(
    instance: &DeviceInstance,
    spec: &DeviceSpec,
) -> Option<Box<dyn DeviceProcessor>> {
    let mut processor: Box<dyn DeviceProcessor> = match spec.type_id.as_str() {
        TOOL_TYPE_ID => Box::new(ToolProcessor::new(spec)),
        EQ_TYPE_ID => Box::new(EqProcessor::new(spec)),
        CHAIN_TYPE_ID => Box::new(ChainProcessor::new(spec)),
        _ => return None,
    };
    for (id, value) in &instance.param_values {
        processor.set_param(*id, value);
    }
    Some(processor)
}

fn param_value(values: &BTreeMap<ParamId, f64>, id: ParamId, default: f64) -> f64 {
    values.get(&id).copied().unwrap_or(default)
}

fn store_value(values: &mut BTreeMap<ParamId, f64>, id: ParamId, value: &ParamValue) {
    let as_f64 = value.as_f64().unwrap_or_default();
    values.insert(id, as_f64);
}

#[derive(Debug, Clone)]
pub struct ToolProcessor {
    values: BTreeMap<ParamId, f64>,
    sample_rate: f64,
    wet: f32,
}

impl ToolProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        Self {
            values: BTreeMap::new(),
            sample_rate: 48_000.0,
            wet: 1.0,
        }
    }
}

impl DeviceProcessor for ToolProcessor {
    fn prepare(&mut self, config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        self.sample_rate = f64::from(config.sample_rate);
        Ok(())
    }

    fn reset(&mut self) {
        self.wet = 1.0;
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        store_value(&mut self.values, param_id, value);
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[crate::devices::NoteEvent],
    ) -> ProcessStatus {
        let input_gain =
            10.0_f32.powf(param_value(&self.values, p_input_gain(), 0.0) as f32 / 20.0);
        let output_gain =
            10.0_f32.powf(param_value(&self.values, p_output_gain(), 0.0) as f32 / 20.0);
        let pan = param_value(&self.values, p_pan(), 0.0) as f32;
        let width = param_value(&self.values, p_width(), 1.0) as f32;
        let polarity_l = param_value(&self.values, p_polarity_l(), 0.0) > 0.5;
        let polarity_r = param_value(&self.values, p_polarity_r(), 0.0) > 0.5;
        let target_wet = param_value(&self.values, p_dry_wet(), 1.0) as f32;
        let dry_wet_step = ((target_wet - self.wet) / audio.len().max(2) as f32).abs();

        let (pan_l, pan_r) = notwig_core::Pan(pan).gains();
        for frame in audio.chunks_exact_mut(2) {
            if self.wet != target_wet {
                self.wet = if self.wet < target_wet {
                    (self.wet + dry_wet_step).min(target_wet)
                } else {
                    (self.wet - dry_wet_step).max(target_wet)
                };
            }
            let dry_l = frame[0];
            let dry_r = frame[1];
            let mut l = dry_l * input_gain;
            let mut r = dry_r * input_gain;
            if polarity_l {
                l = -l;
            }
            if polarity_r {
                r = -r;
            }
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5 * width;
            l = (mid + side) * pan_l;
            r = (mid - side) * pan_r;
            frame[0] = (dry_l * (1.0 - self.wet) + l * self.wet) * output_gain;
            frame[1] = (dry_r * (1.0 - self.wet) + r * self.wet) * output_gain;
        }
        let _ = self.sample_rate;
        ProcessStatus::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EqBandType {
    Bell,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

impl EqBandType {
    fn from_index(index: u32) -> Self {
        match index {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            3 => Self::HighPass,
            4 => Self::LowPass,
            _ => Self::Bell,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn set(&mut self, kind: EqBandType, freq: f64, gain_db: f64, q: f64, sample_rate: f64) {
        let freq = freq.clamp(10.0, sample_rate * 0.49);
        let w0 = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let q = q.max(0.001);
        let alpha = sin_w0 / (2.0 * q);
        let a = 10.0_f64.powf(gain_db / 40.0);

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
                (1.0 + cos_w0) / 2.0,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            EqBandType::LowPass => (
                (1.0 - cos_w0) / 2.0,
                1.0 - cos_w0,
                (1.0 - cos_w0) / 2.0,
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
    fn process(&self, x: f64, state: &mut [f64; 2]) -> f64 {
        let y = self.b0 * x + state[0];
        state[0] = self.b1 * x - self.a1 * y + state[1];
        state[1] = self.b2 * x - self.a2 * y;
        y
    }
}

#[derive(Debug, Clone)]
struct EqBand {
    kind: EqBandType,
    enabled: bool,
    freq: f64,
    gain_db: f64,
    q: f64,
    coeffs: Biquad,
    state: [[f64; 2]; 2],
}

impl EqBand {
    fn new() -> Self {
        Self {
            kind: EqBandType::Bell,
            enabled: true,
            freq: 1_000.0,
            gain_db: 0.0,
            q: 0.707,
            coeffs: Biquad::default(),
            state: [[0.0; 2]; 2],
        }
    }

    fn reset(&mut self) {
        self.state = [[0.0; 2]; 2];
    }
}

#[derive(Debug, Clone)]
pub struct EqProcessor {
    values: BTreeMap<ParamId, f64>,
    bands: [EqBand; 4],
    sample_rate: f64,
}

impl EqProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        Self {
            values: BTreeMap::new(),
            bands: std::array::from_fn(|_| EqBand::new()),
            sample_rate: 48_000.0,
        }
    }

    fn update_band(&mut self, band: usize) {
        let enabled = param_value(&self.values, band_param(band, 0), 1.0) > 0.5;
        let kind =
            EqBandType::from_index(param_value(&self.values, band_param(band, 1), 0.0) as u32);
        let freq = param_value(&self.values, band_param(band, 2), 1_000.0);
        let gain = param_value(&self.values, band_param(band, 3), 0.0);
        let q = param_value(&self.values, band_param(band, 4), 0.707);
        let b = &mut self.bands[band];
        b.enabled = enabled;
        b.kind = kind;
        b.freq = freq;
        b.gain_db = gain;
        b.q = q;
        b.coeffs.set(kind, freq, gain, q, self.sample_rate);
    }
}

impl DeviceProcessor for EqProcessor {
    fn prepare(&mut self, config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        self.sample_rate = f64::from(config.sample_rate);
        for band in &mut self.bands {
            band.reset();
        }
        Ok(())
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        store_value(&mut self.values, param_id, value);
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[crate::devices::NoteEvent],
    ) -> ProcessStatus {
        for band in 0..4 {
            self.update_band(band);
        }
        let output_gain =
            10.0_f32.powf(param_value(&self.values, p_eq_output_gain(), 0.0) as f32 / 20.0);
        for frame in audio.chunks_exact_mut(2) {
            let mut l = f64::from(frame[0]);
            let mut r = f64::from(frame[1]);
            for band in &mut self.bands {
                if !band.enabled {
                    continue;
                }
                l = band.coeffs.process(l, &mut band.state[0]);
                r = band.coeffs.process(r, &mut band.state[1]);
            }
            frame[0] = l as f32 * output_gain;
            frame[1] = r as f32 * output_gain;
        }
        ProcessStatus::default()
    }
}

#[derive(Debug, Clone)]
pub struct ChainProcessor {
    values: BTreeMap<ParamId, f64>,
}

impl ChainProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }
}

impl DeviceProcessor for ChainProcessor {
    fn prepare(&mut self, _config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        Ok(())
    }

    fn reset(&mut self) {}

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        store_value(&mut self.values, param_id, value);
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[crate::devices::NoteEvent],
    ) -> ProcessStatus {
        let input_gain =
            10.0_f32.powf(param_value(&self.values, p_input_gain(), 0.0) as f32 / 20.0);
        let output_gain =
            10.0_f32.powf(param_value(&self.values, p_output_gain(), 0.0) as f32 / 20.0);
        let wet = param_value(&self.values, p_dry_wet(), 1.0) as f32;
        for frame in audio.chunks_exact_mut(2) {
            let dry_l = frame[0];
            let dry_r = frame[1];
            let out_l = dry_l * input_gain * output_gain;
            let out_r = dry_r * input_gain * output_gain;
            frame[0] = dry_l * (1.0 - wet) + out_l * wet;
            frame[1] = dry_r * (1.0 - wet) + out_r * wet;
        }
        ProcessStatus::default()
    }

    fn latency(&self) -> Latency {
        Latency { samples: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{RenderConfig, TransportInfo};
    use notwig_core::TimeMus;

    fn config() -> RenderConfig {
        RenderConfig {
            sample_rate: 48_000,
            max_block_size: 256,
            channels: 2,
            tempo_bpm: 120.0,
        }
    }

    fn ctx(config: RenderConfig) -> ProcessContext<'static> {
        static TRANSPORT: TransportInfo = TransportInfo {
            playhead_mus: TimeMus::zero(),
            loop_start_mus: None,
            loop_end_mus: None,
        };
        ProcessContext {
            config,
            sample_position: 0,
            playing: true,
            tempo_bpm: 120.0,
            transport: &TRANSPORT,
        }
    }

    #[test]
    fn tool_gain_and_polarity() {
        let spec = tool_spec();
        let mut tool = ToolProcessor::new(&spec);
        tool.prepare(config()).unwrap();
        tool.set_param(p_input_gain(), &ParamValue::F64(6.0));
        tool.set_param(p_polarity_l(), &ParamValue::Bool(true));
        let mut audio = vec![0.5, 0.5, 0.5, 0.5];
        tool.process(&ctx(config()), &mut audio, &[]);
        assert!(audio[0] < 0.0);
        assert!(audio[1] > 0.0);
        assert!((audio[0].abs() - 0.7071).abs() < 0.01);
    }

    #[test]
    fn eq_bell_at_unity_is_transparent() {
        let spec = eq_spec();
        let mut eq = EqProcessor::new(&spec);
        eq.prepare(config()).unwrap();
        for band in 0..4 {
            eq.set_param(band_param(band, 0), &ParamValue::Bool(false));
        }
        let mut audio = vec![0.25, -0.25, 0.3, -0.3];
        let original = audio.clone();
        eq.process(&ctx(config()), &mut audio, &[]);
        for (a, b) in audio.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn eq_highpass_attenuates_dc() {
        let spec = eq_spec();
        let mut eq = EqProcessor::new(&spec);
        eq.prepare(config()).unwrap();
        for band in 0..4 {
            eq.set_param(band_param(band, 0), &ParamValue::Bool(false));
        }
        eq.set_param(band_param(0, 0), &ParamValue::Bool(true));
        eq.set_param(band_param(0, 1), &ParamValue::EnumIndex(3));
        eq.set_param(band_param(0, 2), &ParamValue::F64(100.0));
        eq.set_param(band_param(0, 4), &ParamValue::F64(0.707));
        let mut audio = vec![1.0; 4096];
        eq.process(&ctx(config()), &mut audio, &[]);
        assert!(audio[4090].abs() < 0.05);
    }
}
