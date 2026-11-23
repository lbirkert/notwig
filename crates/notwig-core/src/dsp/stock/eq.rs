use std::collections::BTreeMap;

use crate::{Db, ParamId, ParamValue};

use super::super::devices::{DeviceProcessor, DeviceSpec, ProcessContext, ProcessStatus};
use super::super::graph::GraphError;
use super::super::smoothing::SmoothedValue;
use super::biquad::{Biquad, EqBandType};
use super::specs::{band_param, p_eq_output_gain, p_eq_shift};

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
    fn prepare(&mut self, config: super::super::devices::RenderConfig) -> Result<(), GraphError> {
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
        _events: &[super::super::devices::NoteEvent],
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

fn param_value(values: &BTreeMap<ParamId, f64>, id: ParamId, default: f64) -> f64 {
    values.get(&id).copied().unwrap_or(default)
}

fn store_value(values: &mut BTreeMap<ParamId, f64>, id: ParamId, value: &ParamValue) {
    let as_f64 = value.as_f64().unwrap_or_default();
    values.insert(id, as_f64);
}
