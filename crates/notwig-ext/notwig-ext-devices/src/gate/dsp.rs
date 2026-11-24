//! Gate detector, meter ballistics, and sample processing.

use notwig_core::ParamId;
use notwig_ext_api::DeviceMeterSnapshot;

use super::GateDevice;
use super::params::{
    DEFAULT_ATTACK_SECONDS, DEFAULT_HOLD_SECONDS, DEFAULT_RANGE_DB, DEFAULT_RELEASE_SECONDS,
    DEFAULT_THRESHOLD_DB, MAX_ATTACK_SECONDS, MAX_HOLD_SECONDS, MAX_RANGE_DB, MAX_RELEASE_SECONDS,
    MAX_THRESHOLD_DB, MIN_ATTACK_SECONDS, MIN_HOLD_SECONDS, MIN_RANGE_DB, MIN_RELEASE_SECONDS,
    MIN_THRESHOLD_DB, attack_id, hold_id, range_id, release_id, threshold_id,
};
use crate::dynamics::{History, HistorySample, METER_MIN_DB, follow_peak, linear_to_db};

impl Default for GateDevice {
    fn default() -> Self {
        let mut gate = Self {
            sample_rate: 48_000.0,
            channels: 2,
            threshold_db: DEFAULT_THRESHOLD_DB,
            threshold: 0.0,
            attack_seconds: DEFAULT_ATTACK_SECONDS,
            attack_coeff: 0.0,
            hold_seconds: DEFAULT_HOLD_SECONDS,
            hold_samples: 0,
            hold_remaining: 0,
            release_seconds: DEFAULT_RELEASE_SECONDS,
            release_coeff: 0.0,
            range_db: DEFAULT_RANGE_DB,
            floor_gain: 0.0,
            gain: 1.0,
            history: History::new(),
            level_db: METER_MIN_DB,
            meter_left_db: METER_MIN_DB,
            meter_right_db: METER_MIN_DB,
            meter_peak_db: METER_MIN_DB,
            gain_reduction_db: 0.0,
            last_meter_time: 0.0,
            last_history_time: -1.0,
        };
        gate.update_threshold();
        gate.update_timing();
        gate.update_range();
        gate
    }
}

impl GateDevice {
    pub(super) fn update_threshold(&mut self) {
        self.threshold = db_to_gain(self.threshold_db);
    }

    pub(super) fn update_timing(&mut self) {
        self.attack_coeff = time_coeff(self.attack_seconds, self.sample_rate);
        self.release_coeff = time_coeff(self.release_seconds, self.sample_rate);
        self.hold_samples = (self.hold_seconds * self.sample_rate).round() as usize;
    }

    pub(super) fn update_range(&mut self) {
        self.floor_gain = db_to_gain(self.range_db);
    }

    fn reset_meters(&mut self) {
        self.history = History::new();
        self.level_db = METER_MIN_DB;
        self.meter_left_db = METER_MIN_DB;
        self.meter_right_db = METER_MIN_DB;
        self.meter_peak_db = METER_MIN_DB;
        self.gain_reduction_db = 0.0;
        self.last_meter_time = 0.0;
        self.last_history_time = -1.0;
    }

    pub(super) fn update_meters(&mut self, meter: DeviceMeterSnapshot, now: f64) {
        let dt = if self.last_meter_time > 0.0 {
            (now - self.last_meter_time).clamp(1.0 / 240.0, 0.25) as f32
        } else {
            1.0 / 60.0
        };
        self.last_meter_time = now;
        let input_peak = meter.input_peak_l.max(meter.input_peak_r);
        let output_peak = meter.output_peak_l.max(meter.output_peak_r);
        let level_db = follow_peak(self.level_db, linear_to_db(input_peak), 48.0 * dt);
        self.level_db = level_db;
        let gain_db = if input_peak > 1e-6 {
            linear_to_db((output_peak / input_peak).clamp(0.0, 1.0))
        } else {
            self.gain_reduction_db
        };
        self.gain_reduction_db = gain_db;

        self.meter_left_db = follow_peak(
            self.meter_left_db,
            linear_to_db(meter.output_peak_l),
            48.0 * dt,
        );
        self.meter_right_db = follow_peak(
            self.meter_right_db,
            linear_to_db(meter.output_peak_r),
            48.0 * dt,
        );
        let peak_db = self.meter_left_db.max(self.meter_right_db);
        if peak_db >= self.meter_peak_db {
            self.meter_peak_db = peak_db;
        } else {
            self.meter_peak_db = (self.meter_peak_db - 15.0 * dt).max(METER_MIN_DB);
        }

        if self.last_history_time < 0.0 || now - self.last_history_time >= 1.0 / 60.0 {
            self.history.push(HistorySample { level_db, gain_db });
            self.last_history_time = now;
        }
    }
}

fn time_coeff(seconds: f64, sample_rate: f64) -> f32 {
    let samples = (seconds * sample_rate).max(1.0);
    ((1.0 - (-1.0 / samples).exp()) as f32).clamp(0.0, 1.0)
}

fn db_to_gain(db: f64) -> f32 {
    10.0_f64.powf(db / 20.0) as f32
}

pub(super) fn prepare(device: &mut GateDevice, sample_rate: u32, channels: u16) {
    device.sample_rate = f64::from(sample_rate.max(1));
    device.channels = usize::from(channels.max(1));
    device.gain = 1.0;
    device.hold_remaining = 0;
    device.update_timing();
    device.reset_meters();
}

pub(super) fn set_param(device: &mut GateDevice, param_id: ParamId, value: f64) -> bool {
    if param_id == threshold_id() {
        device.threshold_db = value.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
        device.update_threshold();
        true
    } else if param_id == attack_id() {
        device.attack_seconds = value.clamp(MIN_ATTACK_SECONDS, MAX_ATTACK_SECONDS);
        device.update_timing();
        true
    } else if param_id == hold_id() {
        device.hold_seconds = value.clamp(MIN_HOLD_SECONDS, MAX_HOLD_SECONDS);
        device.update_timing();
        true
    } else if param_id == release_id() {
        device.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
        device.update_timing();
        true
    } else if param_id == range_id() {
        device.range_db = value.clamp(MIN_RANGE_DB, MAX_RANGE_DB);
        device.update_range();
        true
    } else {
        false
    }
}

pub(super) fn process(device: &mut GateDevice, samples: &mut [f32]) {
    let channels = device.channels.max(1);
    let threshold = device.threshold;
    let floor_gain = device.floor_gain;
    let attack_coeff = device.attack_coeff;
    let release_coeff = device.release_coeff;
    let hold_samples = device.hold_samples;
    let mut hold = device.hold_remaining;
    let mut gain = device.gain;
    for frame in samples.chunks_mut(channels) {
        let peak = frame
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let target = if peak > threshold {
            hold = hold_samples;
            1.0
        } else if hold > 0 {
            hold -= 1;
            1.0
        } else {
            floor_gain
        };
        let coeff = if target > gain {
            attack_coeff
        } else {
            release_coeff
        };
        gain += (target - gain) * coeff;
        if (target - gain).abs() < 0.000_5 {
            gain = target;
        }
        for sample in frame {
            *sample *= gain;
        }
    }
    device.hold_remaining = hold;
    device.gain = gain;
}

pub(super) fn latency(_device: &GateDevice) -> u32 {
    0
}
