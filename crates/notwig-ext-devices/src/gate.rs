use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_ext_api::{
    Align, DeviceExtension, DeviceMeterSnapshot, METER_MIN_DB, Point, Rect, Size, UiContext, knob,
    paint_stereo_meter, section,
};
use uuid::Uuid;

use crate::dynamics::{
    History, HistorySample, ThresholdCurve, follow_peak, linear_to_db, paint_history_graph,
};

pub const TYPE_ID: &str = "notwig.ext.devices.gate";

const THRESHOLD_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0001;
const ATTACK_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0002;
const HOLD_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0003;
const RELEASE_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0004;
const RANGE_UUID: u128 = 0x4741_5445_0000_0000_0000_0000_0000_0005;

const DEFAULT_THRESHOLD_DB: f64 = -40.0;
const DEFAULT_ATTACK_SECONDS: f64 = 0.005;
const DEFAULT_HOLD_SECONDS: f64 = 0.05;
const DEFAULT_RELEASE_SECONDS: f64 = 0.1;
const DEFAULT_RANGE_DB: f64 = -80.0;

const MIN_THRESHOLD_DB: f64 = -80.0;
const MAX_THRESHOLD_DB: f64 = 0.0;
const MIN_ATTACK_SECONDS: f64 = 0.001;
const MAX_ATTACK_SECONDS: f64 = 0.1;
const MIN_HOLD_SECONDS: f64 = 0.0;
const MAX_HOLD_SECONDS: f64 = 0.5;
const MIN_RELEASE_SECONDS: f64 = 0.01;
const MAX_RELEASE_SECONDS: f64 = 1.0;
const MIN_RANGE_DB: f64 = -80.0;
const MAX_RANGE_DB: f64 = 0.0;

const METER_WIDTH: f32 = 40.0;
const GRAPH_HEIGHT: f32 = 116.0;
const KNOB_HEIGHT: f32 = 58.0;
const KNOB_GAP: f32 = 2.0;

#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

#[must_use]
pub fn attack_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_UUID))
}

#[must_use]
pub fn hold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(HOLD_UUID))
}

#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

#[must_use]
pub fn range_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RANGE_UUID))
}

fn threshold_norm(value_db: f64) -> f32 {
    ((value_db - MIN_THRESHOLD_DB) / (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)) as f32
}

fn attack_norm(seconds: f64) -> f32 {
    ((seconds - MIN_ATTACK_SECONDS) / (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)) as f32
}

fn hold_norm(seconds: f64) -> f32 {
    ((seconds - MIN_HOLD_SECONDS) / (MAX_HOLD_SECONDS - MIN_HOLD_SECONDS)) as f32
}

fn release_norm(seconds: f64) -> f32 {
    ((seconds - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

fn range_norm(value_db: f64) -> f32 {
    ((value_db - MIN_RANGE_DB) / (MAX_RANGE_DB - MIN_RANGE_DB)) as f32
}

fn time_coeff(seconds: f64, sample_rate: f64) -> f32 {
    let samples = (seconds * sample_rate).max(1.0);
    ((1.0 - (-1.0 / samples).exp()) as f32).clamp(0.0, 1.0)
}

fn db_to_gain(db: f64) -> f32 {
    10.0_f64.powf(db / 20.0) as f32
}

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
}

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
        };
        gate.update_threshold();
        gate.update_timing();
        gate.update_range();
        gate
    }
}

impl GateDevice {
    fn update_threshold(&mut self) {
        self.threshold = db_to_gain(self.threshold_db);
    }

    fn update_timing(&mut self) {
        self.attack_coeff = time_coeff(self.attack_seconds, self.sample_rate);
        self.release_coeff = time_coeff(self.release_seconds, self.sample_rate);
        self.hold_samples = (self.hold_seconds * self.sample_rate).round() as usize;
    }

    fn update_range(&mut self) {
        self.floor_gain = db_to_gain(self.range_db);
    }

    fn reset_meters(&mut self) {
        self.history = History::new();
        self.level_db = METER_MIN_DB;
        self.meter_left_db = METER_MIN_DB;
        self.meter_right_db = METER_MIN_DB;
        self.meter_peak_db = METER_MIN_DB;
        self.gain_reduction_db = 0.0;
    }

    fn update_meters(&mut self, meter: DeviceMeterSnapshot) {
        let input_peak = meter.input_peak_l.max(meter.input_peak_r);
        let output_peak = meter.output_peak_l.max(meter.output_peak_r);
        let level_db = follow_peak(self.level_db, linear_to_db(input_peak), 0.8);
        self.level_db = level_db;
        let gain_db = if input_peak > 1e-6 {
            linear_to_db((output_peak / input_peak).clamp(0.0, 1.0))
        } else {
            self.gain_reduction_db
        };
        self.gain_reduction_db = gain_db;
        self.history.push(HistorySample { level_db, gain_db });

        self.meter_left_db =
            follow_peak(self.meter_left_db, linear_to_db(meter.output_peak_l), 0.8);
        self.meter_right_db =
            follow_peak(self.meter_right_db, linear_to_db(meter.output_peak_r), 0.8);
        let peak_db = self.meter_left_db.max(self.meter_right_db);
        if peak_db >= self.meter_peak_db {
            self.meter_peak_db = peak_db;
        } else {
            self.meter_peak_db = (self.meter_peak_db - 0.25).max(METER_MIN_DB);
        }
    }
}

impl DeviceExtension for GateDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
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
        }
    }

    fn draw(&mut self, ui: &mut UiContext<'_>) {
        self.update_meters(ui.device_meter());
        section(ui, "Gate");

        let width = ui.available_rect().width();
        let spacing = ui.style().spacing.width;
        ui.horizontal(|ui| {
            let graph_width = (width - METER_WIDTH - spacing).max(60.0);
            let graph_rect = ui.allocate(Size::new(graph_width, GRAPH_HEIGHT));
            let theme = ui.theme();
            paint_history_graph(
                ui.painter(),
                &theme,
                graph_rect,
                &self.history,
                self.threshold_db as f32,
                ThresholdCurve::Fixed,
                true,
            );
            let font = ui.mono_font();
            let closed = self.gain_reduction_db < -1.0;
            let text = format!(
                "{} {:.0} dB",
                if closed { "CLOSED" } else { "OPEN" },
                self.gain_reduction_db
            );
            ui.painter().text(
                Point::new(graph_rect.min.x + 6.0, graph_rect.min.y + 5.0),
                &text,
                font,
                if closed {
                    theme.text_muted
                } else {
                    theme.accent
                },
                Align::left_top(),
            );

            let meter_rect = ui.allocate(Size::new(METER_WIDTH, GRAPH_HEIGHT));
            let to_linear = |db: f32| 10.0_f32.powf(db / 20.0);
            paint_stereo_meter(
                ui.painter(),
                &theme,
                meter_rect,
                to_linear(self.meter_left_db),
                to_linear(self.meter_right_db),
                to_linear(self.meter_peak_db),
            );
        });

        let row = ui.allocate(Size::new(width, KNOB_HEIGHT));
        let count = 5.0_f32;
        let knob_width = ((width - KNOB_GAP * (count - 1.0)) / count).max(24.0);
        let cells = [
            (
                "Thr",
                threshold_norm(self.threshold_db),
                threshold_norm(DEFAULT_THRESHOLD_DB),
            ),
            (
                "Atk",
                attack_norm(self.attack_seconds),
                attack_norm(DEFAULT_ATTACK_SECONDS),
            ),
            (
                "Hold",
                hold_norm(self.hold_seconds),
                hold_norm(DEFAULT_HOLD_SECONDS),
            ),
            (
                "Rel",
                release_norm(self.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
            ),
            (
                "Rng",
                range_norm(self.range_db),
                range_norm(DEFAULT_RANGE_DB),
            ),
        ];
        let mut changes = [None; 5];
        for (index, (label, value, default)) in cells.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                Point::new(
                    row.min.x + index as f32 * (knob_width + KNOB_GAP),
                    row.min.y,
                ),
                Size::new(knob_width, KNOB_HEIGHT),
            );
            changes[index] = knob(
                ui,
                ui.id(("gate", index)),
                rect,
                value,
                default,
                false,
                label,
            );
        }

        if let Some(norm) = changes[0] {
            self.threshold_db =
                MIN_THRESHOLD_DB + f64::from(norm) * (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB);
            self.update_threshold();
            ui.set_param_normalized(threshold_id(), norm);
        }
        if let Some(norm) = changes[1] {
            self.attack_seconds =
                MIN_ATTACK_SECONDS + f64::from(norm) * (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS);
            self.update_timing();
            ui.set_param_normalized(attack_id(), norm);
        }
        if let Some(norm) = changes[2] {
            self.hold_seconds =
                MIN_HOLD_SECONDS + f64::from(norm) * (MAX_HOLD_SECONDS - MIN_HOLD_SECONDS);
            self.update_timing();
            ui.set_param_normalized(hold_id(), norm);
        }
        if let Some(norm) = changes[3] {
            self.release_seconds =
                MIN_RELEASE_SECONDS + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS);
            self.update_timing();
            ui.set_param_normalized(release_id(), norm);
        }
        if let Some(norm) = changes[4] {
            self.range_db = MIN_RANGE_DB + f64::from(norm) * (MAX_RANGE_DB - MIN_RANGE_DB);
            self.update_range();
            ui.set_param_normalized(range_id(), norm);
        }

        ui.request_repaint();
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.channels = usize::from(channels.max(1));
        self.gain = 1.0;
        self.hold_remaining = 0;
        self.update_timing();
        self.reset_meters();
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == threshold_id() {
            self.threshold_db = value.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
            self.update_threshold();
            true
        } else if param_id == attack_id() {
            self.attack_seconds = value.clamp(MIN_ATTACK_SECONDS, MAX_ATTACK_SECONDS);
            self.update_timing();
            true
        } else if param_id == hold_id() {
            self.hold_seconds = value.clamp(MIN_HOLD_SECONDS, MAX_HOLD_SECONDS);
            self.update_timing();
            true
        } else if param_id == release_id() {
            self.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
            self.update_timing();
            true
        } else if param_id == range_id() {
            self.range_db = value.clamp(MIN_RANGE_DB, MAX_RANGE_DB);
            self.update_range();
            true
        } else {
            false
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        let channels = self.channels.max(1);
        let threshold = self.threshold;
        let floor_gain = self.floor_gain;
        let attack_coeff = self.attack_coeff;
        let release_coeff = self.release_coeff;
        let hold_samples = self.hold_samples;
        let mut hold = self.hold_remaining;
        let mut gain = self.gain;
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
        self.hold_remaining = hold;
        self.gain = gain;
    }

    fn latency(&self) -> u32 {
        0
    }
}
