use std::time::Duration;

use egui::{Align2, FontId, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, knob, palette, section};
use uuid::Uuid;

use crate::dynamics::{
    History, HistorySample, METER_MIN_DB, ThresholdCurve, follow_peak, linear_to_db,
    paint_history_graph,
};
use crate::ui::knob_label;

/// Device type id for the stock limiter.
pub const TYPE_ID: &str = "notwig.ext.devices.limiter";

/// Fixed lookahead in samples (~1.33 ms at 48 kHz).
pub const LOOKAHEAD_SAMPLES: u32 = 64;

const THRESHOLD_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0001;
const RELEASE_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0002;

const DEFAULT_THRESHOLD_DB: f64 = -6.0;
const DEFAULT_RELEASE_SECONDS: f64 = 0.1;

const MIN_THRESHOLD_DB: f64 = -24.0;
const MAX_THRESHOLD_DB: f64 = 0.0;
const MIN_RELEASE_SECONDS: f64 = 0.01;
const MAX_RELEASE_SECONDS: f64 = 0.5;

/// Returns the threshold parameter id.
#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

/// Returns the release parameter id.
#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

fn threshold_norm(value_db: f64) -> f32 {
    ((value_db - MIN_THRESHOLD_DB) / (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)) as f32
}

fn release_norm(seconds: f64) -> f32 {
    ((seconds - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

/// Stock lookahead limiter device.
#[derive(Debug, Clone)]
pub struct LimiterDevice {
    sample_rate: f64,
    channels: usize,
    threshold_db: f64,
    threshold: f32,
    release_seconds: f64,
    release_coeff: f32,
    gain: f32,
    lookahead: usize,
    delay: Vec<f32>,
    delay_pos: usize,
    gain_history: Vec<f32>,
    gain_pos: usize,
    history: History,
    level_db: f32,
    meter_left_db: f32,
    meter_right_db: f32,
    meter_peak_db: f32,
    gain_reduction_db: f32,
    last_meter_time: f64,
    last_history_time: f64,
}

impl Default for LimiterDevice {
    fn default() -> Self {
        let mut limiter = Self {
            sample_rate: 48_000.0,
            channels: 2,
            threshold_db: DEFAULT_THRESHOLD_DB,
            threshold: 0.0,
            release_seconds: DEFAULT_RELEASE_SECONDS,
            release_coeff: 0.0,
            gain: 1.0,
            lookahead: LOOKAHEAD_SAMPLES as usize,
            delay: Vec::new(),
            delay_pos: 0,
            gain_history: Vec::new(),
            gain_pos: 0,
            history: History::new(),
            level_db: METER_MIN_DB,
            meter_left_db: METER_MIN_DB,
            meter_right_db: METER_MIN_DB,
            meter_peak_db: METER_MIN_DB,
            gain_reduction_db: 0.0,
            last_meter_time: 0.0,
            last_history_time: -1.0,
        };
        limiter.update_threshold();
        limiter.update_release();
        limiter
    }
}

impl LimiterDevice {
    fn update_threshold(&mut self) {
        self.threshold = 10.0_f64.powf(self.threshold_db / 20.0) as f32;
    }

    fn update_release(&mut self) {
        let samples = (self.release_seconds * self.sample_rate).max(1.0);
        self.release_coeff = (1.0 - (-1.0 / samples).exp()) as f32;
        self.release_coeff = self.release_coeff.clamp(0.0, 1.0);
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

    fn update_meters(&mut self, meter: DeviceMeterSnapshot, now: f64) {
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

        if self.last_history_time < 0.0 || now - self.last_history_time >= 1.0 / 30.0 {
            self.history.push(HistorySample { level_db, gain_db });
            self.last_history_time = now;
        }
    }
}

impl DeviceExtension for LimiterDevice {
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
                    release_id(),
                    "Release",
                    Unit::Seconds,
                    MIN_RELEASE_SECONDS,
                    MAX_RELEASE_SECONDS,
                    DEFAULT_RELEASE_SECONDS,
                ),
            ],
            latency_samples: LOOKAHEAD_SAMPLES,
            name: "Limiter".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(240.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        let now = ui.input(|input| input.time);
        self.update_meters(meter, now);
        section(ui, "Limiter");

        let width = ui.available_width();
        let meter_width = 40.0;
        let graph_height = 116.0;
        ui.horizontal(|ui| {
            let graph_width = (width - meter_width - 6.0).max(60.0);
            let (graph_rect, _) =
                ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
            paint_history_graph(
                ui.painter(),
                graph_rect,
                &self.history,
                self.threshold_db as f32,
                ThresholdCurve::FollowGain,
                false,
            );
            ui.painter().with_clip_rect(graph_rect).text(
                graph_rect.min + egui::vec2(6.0, 5.0),
                Align2::LEFT_TOP,
                format!("GR {:.1} dB", self.gain_reduction_db),
                FontId::monospace(9.0),
                palette::TEXT_MUTED,
            );

            let (meter_rect, _) =
                ui.allocate_exact_size(Vec2::new(meter_width, graph_height), Sense::hover());
            let to_linear = |db: f32| 10.0_f32.powf(db / 20.0);
            draw_stereo_meter_with_peak(
                ui.painter(),
                meter_rect,
                to_linear(self.meter_left_db),
                to_linear(self.meter_right_db),
                Some(to_linear(self.meter_peak_db)),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let knob_width = 78.0;
            let knob_height = 58.0;

            let (threshold_rect, _) =
                ui.allocate_exact_size(Vec2::new(knob_width, knob_height), Sense::hover());
            if let Some(norm) = knob(
                ui,
                ui.make_persistent_id(("ext-limiter", "threshold")),
                threshold_rect,
                threshold_norm(self.threshold_db),
                threshold_norm(DEFAULT_THRESHOLD_DB),
                true,
                "Threshold",
            ) {
                self.threshold_db =
                    MIN_THRESHOLD_DB + f64::from(norm) * (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB);
                self.update_threshold();
                panel_frame::set_param_normalized(ui, threshold_id(), norm);
            }
            knob_label(ui, threshold_rect, "Threshold");

            let (release_rect, _) =
                ui.allocate_exact_size(Vec2::new(knob_width, knob_height), Sense::hover());
            if let Some(norm) = knob(
                ui,
                ui.make_persistent_id(("ext-limiter", "release")),
                release_rect,
                release_norm(self.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
                false,
                "Release",
            ) {
                self.release_seconds = MIN_RELEASE_SECONDS
                    + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS);
                self.update_release();
                panel_frame::set_param_normalized(ui, release_id(), norm);
            }
            knob_label(ui, release_rect, "Release");
        });

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.channels = usize::from(channels.max(1));
        self.gain = 1.0;
        self.lookahead = LOOKAHEAD_SAMPLES as usize;
        self.delay = vec![0.0; self.lookahead * self.channels];
        self.delay_pos = 0;
        self.gain_history = vec![1.0; self.lookahead + 1];
        self.gain_pos = 0;
        self.update_release();
        self.reset_meters();
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == threshold_id() {
            self.threshold_db = value.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
            self.update_threshold();
            true
        } else if param_id == release_id() {
            self.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
            self.update_release();
            true
        } else {
            false
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        let channels = self.channels.max(1);
        let lookahead = self.lookahead;
        if lookahead == 0
            || self.delay.len() < lookahead * channels
            || self.gain_history.len() < lookahead + 1
        {
            return;
        }
        let threshold = self.threshold;
        let release_coeff = self.release_coeff;
        let mut gain = self.gain;
        let mut delay_pos = self.delay_pos;
        let mut gain_pos = self.gain_pos;
        for frame in samples.chunks_mut(channels) {
            let peak = frame
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let target = if peak > threshold && peak > 0.0 {
                threshold / peak
            } else {
                1.0
            };

            // Hold the required gain for the full lookahead window so the
            // reduction is already active when the delayed peak arrives.
            self.gain_history[gain_pos] = target;
            gain_pos = (gain_pos + 1) % self.gain_history.len();
            let window_gain = self.gain_history.iter().copied().fold(1.0_f32, f32::min);
            if window_gain < gain {
                gain = window_gain;
            } else {
                gain += (window_gain - gain) * release_coeff;
            }

            for (channel, sample) in frame.iter_mut().enumerate() {
                let index = delay_pos * channels + channel;
                let delayed = self.delay[index];
                self.delay[index] = *sample;
                *sample = delayed * gain;
            }
            delay_pos = (delay_pos + 1) % lookahead;
        }
        self.gain = gain;
        self.delay_pos = delay_pos;
        self.gain_pos = gain_pos;
    }

    fn latency(&self) -> u32 {
        LOOKAHEAD_SAMPLES
    }
}
