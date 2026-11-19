use std::collections::BTreeMap;

use notwig_core::{Db, DeviceId, DeviceInstance, DeviceKind, ParamId, ParamValue, SchemaVersion};
use uuid::Uuid;

use crate::devices::{
    DeviceProcessor, DeviceSpec, Latency, Port, PortDir, ProcessContext, ProcessStatus, SignalType,
};
use crate::graph::GraphError;
use crate::params::{ParamSpec, Unit};
use crate::registry::DeviceRegistry;
use crate::smoothing::SmoothedValue;

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
            ParamSpec::boolean(p_polarity_r(), "Polarity R", false),
            dry_wet_param(p_dry_wet()),
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
pub fn instantiate_stock_params(
    type_id: &str,
    spec: &DeviceSpec,
    param_values: &BTreeMap<ParamId, ParamValue>,
) -> Option<Box<dyn DeviceProcessor>> {
    let mut processor: Box<dyn DeviceProcessor> = match type_id {
        TOOL_TYPE_ID => Box::new(ToolProcessor::new(spec)),
        EQ_TYPE_ID => Box::new(EqProcessor::new(spec)),
        CHAIN_TYPE_ID => Box::new(ChainProcessor::new(spec)),
        _ => return None,
    };
    for param in &spec.params {
        processor.set_param(param.id, &param.default);
    }
    for (id, value) in param_values {
        processor.set_param(*id, value);
    }
    Some(processor)
}

#[must_use]
pub fn instantiate_stock(
    instance: &DeviceInstance,
    spec: &DeviceSpec,
) -> Option<Box<dyn DeviceProcessor>> {
    let params: BTreeMap<ParamId, ParamValue> = instance
        .param_values
        .iter()
        .map(|(id, value)| (*id, value.clone()))
        .collect();
    instantiate_stock_params(&instance.type_id, spec, &params)
}

fn param_value(values: &BTreeMap<ParamId, f64>, id: ParamId, default: f64) -> f64 {
    values.get(&id).copied().unwrap_or(default)
}

fn store_value(values: &mut BTreeMap<ParamId, f64>, id: ParamId, value: &ParamValue) {
    let as_f64 = value.as_f64().unwrap_or_default();
    values.insert(id, as_f64);
}

#[derive(Debug, Clone)]
struct ToolSmoothing {
    input_gain: SmoothedValue,
    output_gain: SmoothedValue,
    pan: SmoothedValue,
    width: SmoothedValue,
    dry_wet: SmoothedValue,
    polarity_l: SmoothedValue,
    polarity_r: SmoothedValue,
    swap_lr: SmoothedValue,
    sample_rate: f64,
}

impl ToolSmoothing {
    fn new(sample_rate: f64) -> Self {
        Self {
            input_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            output_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            pan: SmoothedValue::new(0.0, sample_rate, 15.0),
            width: SmoothedValue::new(1.0, sample_rate, 15.0),
            dry_wet: SmoothedValue::new(1.0, sample_rate, 15.0),
            polarity_l: SmoothedValue::new(1.0, sample_rate, 8.0),
            polarity_r: SmoothedValue::new(1.0, sample_rate, 8.0),
            swap_lr: SmoothedValue::new(0.0, sample_rate, 8.0),
            sample_rate,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.input_gain.set_sample_rate(self.sample_rate, 15.0);
        self.output_gain.set_sample_rate(self.sample_rate, 15.0);
        self.pan.set_sample_rate(self.sample_rate, 15.0);
        self.width.set_sample_rate(self.sample_rate, 15.0);
        self.dry_wet.set_sample_rate(self.sample_rate, 15.0);
        self.polarity_l.set_sample_rate(self.sample_rate, 8.0);
        self.polarity_r.set_sample_rate(self.sample_rate, 8.0);
        self.swap_lr.set_sample_rate(self.sample_rate, 8.0);
    }

    fn snap(&mut self) {
        self.input_gain.snap();
        self.output_gain.snap();
        self.pan.snap();
        self.width.snap();
        self.dry_wet.snap();
        self.polarity_l.snap();
        self.polarity_r.snap();
        self.swap_lr.snap();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        let value = value.as_f64().unwrap_or(0.0);
        if param_id == p_input_gain() {
            self.input_gain.set_target(Db(value as f32).to_gain_f64());
        } else if param_id == p_output_gain() {
            self.output_gain.set_target(Db(value as f32).to_gain_f64());
        } else if param_id == p_pan() {
            self.pan.set_target(value.clamp(-1.0, 1.0));
        } else if param_id == p_width() {
            self.width.set_target(value.clamp(0.0, 2.0));
        } else if param_id == p_polarity_l() {
            self.polarity_l
                .set_target(if value > 0.5 { -1.0 } else { 1.0 });
        } else if param_id == p_polarity_r() {
            self.polarity_r
                .set_target(if value > 0.5 { -1.0 } else { 1.0 });
        } else if param_id == p_dry_wet() {
            self.dry_wet.set_target(value.clamp(0.0, 1.0));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolProcessor {
    smooth: ToolSmoothing,
}

impl ToolProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        Self {
            smooth: ToolSmoothing::new(48_000.0),
        }
    }
}

impl DeviceProcessor for ToolProcessor {
    fn prepare(&mut self, config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        self.smooth.set_sample_rate(f64::from(config.sample_rate));
        self.smooth.snap();
        Ok(())
    }

    fn reset(&mut self) {
        self.smooth.snap();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        self.smooth.set_param(param_id, value);
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[crate::devices::NoteEvent],
    ) -> ProcessStatus {
        for frame in audio.chunks_exact_mut(2) {
            let input_gain = self.smooth.input_gain.next() as f32;
            let output_gain = self.smooth.output_gain.next() as f32;
            let pan = self.smooth.pan.next() as f32;
            let width = self.smooth.width.next() as f32;
            let wet = self.smooth.dry_wet.next() as f32;
            let polarity_l = self.smooth.polarity_l.next() as f32;
            let polarity_r = self.smooth.polarity_r.next() as f32;
            let swap = self.smooth.swap_lr.next() as f32;
            let (pan_l, pan_r) = notwig_core::Pan(pan).gains();
            let dry_l = frame[0];
            let dry_r = frame[1];
            let mut l = dry_l * input_gain * polarity_l;
            let mut r = dry_r * input_gain * polarity_r;
            if swap > 0.0 {
                let swapped_l = l * (1.0 - swap) + r * swap;
                let swapped_r = r * (1.0 - swap) + l * swap;
                l = swapped_l;
                r = swapped_r;
            }
            let mid = f32::midpoint(l, r);
            let side = (l - r) * 0.5 * width;
            l = (mid + side) * pan_l;
            r = (mid - side) * pan_r;
            frame[0] = (dry_l * (1.0 - wet) + l * wet) * output_gain;
            frame[1] = (dry_r * (1.0 - wet) + r * wet) * output_gain;
        }
        ProcessStatus::default()
    }
}

#[must_use]
pub fn eq_magnitude_response(
    values: &BTreeMap<ParamId, ParamValue>,
    sample_rate: f64,
    frequency: f64,
) -> f64 {
    let mut magnitude = 1.0_f64;
    let shift = values
        .get(&p_eq_shift())
        .and_then(ParamValue::as_f64)
        .unwrap_or(0.0)
        / 12.0;
    for band in 0..4 {
        let enabled = matches!(
            values.get(&band_param(band, 0)),
            Some(ParamValue::Bool(true))
        );
        if !enabled {
            continue;
        }
        let kind = EqBandType::from_index(
            values
                .get(&band_param(band, 1))
                .and_then(ParamValue::as_f64)
                .unwrap_or(0.0) as u32,
        );
        let freq = values
            .get(&band_param(band, 2))
            .and_then(ParamValue::as_f64)
            .unwrap_or(1_000.0)
            * 2.0_f64.powf(shift);
        let gain = values
            .get(&band_param(band, 3))
            .and_then(ParamValue::as_f64)
            .unwrap_or(0.0);
        let q = values
            .get(&band_param(band, 4))
            .and_then(ParamValue::as_f64)
            .unwrap_or(0.707);
        let mut coeffs = Biquad::default();
        coeffs.set(kind, freq, gain, q, sample_rate);
        let w = 2.0 * std::f64::consts::PI * frequency / sample_rate;
        let (sin_w, cos_w) = w.sin_cos();
        let cos_two_w = (2.0 * w).cos();
        let sin_two_w = (2.0 * w).sin();
        let num_re = coeffs.b0 + coeffs.b1 * cos_w + coeffs.b2 * cos_two_w;
        let num_im = -(coeffs.b1 * sin_w + coeffs.b2 * sin_two_w);
        let den_re = 1.0 + coeffs.a1 * cos_w + coeffs.a2 * cos_two_w;
        let den_im = -(coeffs.a1 * sin_w + coeffs.a2 * sin_two_w);
        let num = (num_re * num_re + num_im * num_im).sqrt();
        let den = (den_re * den_re + den_im * den_im).sqrt();
        if den > 1e-12 {
            magnitude *= num / den;
        }
    }
    magnitude
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            b0: self.b0 + (other.b0 - self.b0) * t,
            b1: self.b1 + (other.b1 - self.b1) * t,
            b2: self.b2 + (other.b2 - self.b2) * t,
            a1: self.a1 + (other.a1 - self.a1) * t,
            a2: self.a2 + (other.a2 - self.a2) * t,
        }
    }

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
    fn process(&self, x: f64, state: &mut [f64; 2]) -> f64 {
        let y = self.b0 * x + state[0];
        state[0] = self.b1 * x - self.a1 * y + state[1];
        state[1] = self.b2 * x - self.a2 * y;
        y
    }
}

#[derive(Debug, Clone)]
struct EqBand {
    current: Biquad,
    target: Biquad,
    ramp_remaining: u32,
    state: [[f64; 2]; 2],
    enabled_mix: SmoothedValue,
}

impl EqBand {
    fn new(sample_rate: f64) -> Self {
        Self {
            current: Biquad::identity(),
            target: Biquad::identity(),
            ramp_remaining: 0,
            state: [[0.0; 2]; 2],
            enabled_mix: SmoothedValue::new(1.0, sample_rate, 10.0),
        }
    }

    fn reset(&mut self) {
        self.state = [[0.0; 2]; 2];
        self.enabled_mix.snap();
    }

    fn set_target(&mut self, target: Biquad, enabled: bool, ramp_samples: u32) {
        if target != self.target {
            self.target = target;
            self.ramp_remaining = ramp_samples.max(1);
        }
        self.enabled_mix.set_target(if enabled { 1.0 } else { 0.0 });
    }

    #[inline]
    fn next_coefficients(&mut self) -> Biquad {
        if self.ramp_remaining > 0 {
            let step = 1.0 / f64::from(self.ramp_remaining);
            self.current = self.current.lerp(self.target, step);
            self.ramp_remaining -= 1;
        }
        self.current
    }
}

#[derive(Debug, Clone)]
pub struct EqProcessor {
    values: BTreeMap<ParamId, f64>,
    bands: [EqBand; 4],
    sample_rate: f64,
    ramp_samples: u32,
    output_gain: SmoothedValue,
}

impl EqProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        let sample_rate = 48_000.0;
        Self {
            values: BTreeMap::new(),
            bands: std::array::from_fn(|_| EqBand::new(sample_rate)),
            sample_rate,
            ramp_samples: 480,
            output_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
        }
    }

    fn update_band(&mut self, band: usize) {
        let enabled = param_value(&self.values, band_param(band, 0), 0.0) > 0.5;
        let kind =
            EqBandType::from_index(param_value(&self.values, band_param(band, 1), 0.0) as u32);
        let shift = param_value(&self.values, p_eq_shift(), 0.0) / 12.0;
        let freq = param_value(&self.values, band_param(band, 2), 1_000.0) * 2.0_f64.powf(shift);
        let gain = param_value(&self.values, band_param(band, 3), 0.0);
        let q = param_value(&self.values, band_param(band, 4), 0.707);
        let mut target = Biquad::default();
        target.set(kind, freq, gain, q, self.sample_rate);
        self.bands[band].set_target(target, enabled, self.ramp_samples);
    }
}

impl DeviceProcessor for EqProcessor {
    fn prepare(&mut self, config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        self.sample_rate = f64::from(config.sample_rate).max(1.0);
        self.ramp_samples = ((self.sample_rate * 0.010).round() as u32).max(1);
        self.output_gain.set_sample_rate(self.sample_rate, 15.0);
        self.output_gain.snap();
        for band in 0..4 {
            self.update_band(band);
        }
        for band in &mut self.bands {
            band.enabled_mix.set_sample_rate(self.sample_rate, 10.0);
            band.current = band.target;
            band.ramp_remaining = 0;
            band.enabled_mix.snap();
            band.state = [[0.0; 2]; 2];
        }
        Ok(())
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.output_gain.snap();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        store_value(&mut self.values, param_id, value);
        if param_id == p_eq_output_gain() {
            let gain = value.as_f64().unwrap_or(0.0);
            self.output_gain.set_target(Db(gain as f32).to_gain_f64());
        }
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
        for frame in audio.chunks_exact_mut(2) {
            let output_gain = self.output_gain.next() as f32;
            let mut l = f64::from(frame[0]);
            let mut r = f64::from(frame[1]);
            for band in &mut self.bands {
                let coefficients = band.next_coefficients();
                let mix = band.enabled_mix.next();
                let filtered_l = coefficients.process(l, &mut band.state[0]);
                let filtered_r = coefficients.process(r, &mut band.state[1]);
                l += (filtered_l - l) * mix;
                r += (filtered_r - r) * mix;
            }
            frame[0] = l as f32 * output_gain;
            frame[1] = r as f32 * output_gain;
        }
        ProcessStatus::default()
    }
}

pub struct ChainProcessor {
    wet_gain: SmoothedValue,
    output_gain: SmoothedValue,
    dry_wet: SmoothedValue,
    sample_rate: f64,
    dry: Vec<f32>,
    children: Vec<(DeviceId, Box<dyn DeviceProcessor>)>,
}

impl ChainProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        let sample_rate = 48_000.0;
        Self {
            wet_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            output_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            dry_wet: SmoothedValue::new(1.0, sample_rate, 15.0),
            sample_rate,
            dry: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, device_id: DeviceId, processor: Box<dyn DeviceProcessor>) {
        self.children.push((device_id, processor));
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.wet_gain.set_sample_rate(self.sample_rate, 15.0);
        self.output_gain.set_sample_rate(self.sample_rate, 15.0);
        self.dry_wet.set_sample_rate(self.sample_rate, 15.0);
    }
}

impl DeviceProcessor for ChainProcessor {
    fn prepare(&mut self, config: crate::devices::RenderConfig) -> Result<(), GraphError> {
        self.set_sample_rate(f64::from(config.sample_rate));
        self.wet_gain.snap();
        self.output_gain.snap();
        self.dry_wet.snap();
        self.dry.resize(config.max_block_size.max(1) * 2, 0.0);
        for (_, child) in &mut self.children {
            child.prepare(config)?;
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.wet_gain.snap();
        self.output_gain.snap();
        self.dry_wet.snap();
        self.dry.fill(0.0);
        for (_, child) in &mut self.children {
            child.reset();
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        let db = value.as_f64().unwrap_or(0.0);
        if param_id == p_input_gain() {
            self.wet_gain.set_target(Db(db as f32).to_gain_f64());
        } else if param_id == p_output_gain() {
            self.output_gain.set_target(Db(db as f32).to_gain_f64());
        } else if param_id == p_dry_wet() {
            self.dry_wet.set_target(db.clamp(0.0, 1.0));
        }
    }

    fn set_nested_param(
        &mut self,
        device_id: DeviceId,
        param_id: ParamId,
        value: &ParamValue,
    ) -> bool {
        for (child_id, child) in &mut self.children {
            if *child_id == device_id {
                child.set_param(param_id, value);
                return true;
            }
            if child.set_nested_param(device_id, param_id, value) {
                return true;
            }
        }
        false
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[crate::devices::NoteEvent],
    ) -> ProcessStatus {
        if self.dry.len() < audio.len() {
            self.dry.resize(audio.len(), 0.0);
        }
        self.dry[..audio.len()].copy_from_slice(audio);
        for (_, child) in &mut self.children {
            child.process(ctx, audio, events);
        }
        for (frame, dry) in audio.chunks_exact_mut(2).zip(self.dry.chunks_exact(2)) {
            let wet_gain = self.wet_gain.next() as f32;
            let output = self.output_gain.next() as f32;
            let mix = self.dry_wet.next() as f32;
            for channel in 0..2 {
                let dry = dry[channel];
                let wet = frame[channel] * wet_gain;
                frame[channel] = (dry * (1.0 - mix) + wet * mix) * output;
            }
        }
        ProcessStatus::default()
    }

    fn latency(&self) -> Latency {
        let samples = self
            .children
            .iter()
            .map(|(_, child)| child.latency().samples)
            .fold(0u32, u32::saturating_add);
        Latency { samples }
    }
}
