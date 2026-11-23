//! Stock noise gate device.
//!
//! [`params`] owns ids, ranges, and normalization; [`spec`] builds the device
//! spec; [`dsp`] owns detector state, meters, and sample processing; [`panel`]
//! draws the egui panel.

use notwig_core::ParamId;
use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui};

use crate::dynamics::History;

mod dsp;
mod panel;
mod params;
mod spec;

pub use params::{attack_id, hold_id, range_id, release_id, threshold_id};

/// Device type id for the stock gate.
pub const TYPE_ID: &str = "notwig.ext.devices.gate";

/// Stock noise gate device.
#[derive(Debug, Clone)]
pub struct GateDevice {
    sample_rate: f64,
    channels: usize,
    threshold_db: f64,
    threshold: f32,
    attack_seconds: f64,
    attack_coeff: f32,
    hold_seconds: f64,
    hold_samples: usize,
    hold_remaining: usize,
    release_seconds: f64,
    release_coeff: f32,
    range_db: f64,
    floor_gain: f32,
    gain: f32,
    history: History,
    level_db: f32,
    meter_left_db: f32,
    meter_right_db: f32,
    meter_peak_db: f32,
    gain_reduction_db: f32,
    last_meter_time: f64,
    last_history_time: f64,
}

impl DeviceExtension for GateDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        spec::spec()
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        panel::draw(self, ui, meter);
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        dsp::prepare(self, sample_rate, channels);
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        dsp::set_param(self, param_id, value)
    }

    fn process(&mut self, samples: &mut [f32]) {
        dsp::process(self, samples);
    }

    fn latency(&self) -> u32 {
        dsp::latency(self)
    }
}
