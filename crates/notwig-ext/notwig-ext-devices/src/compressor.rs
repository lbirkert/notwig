//! Stereo-linked compressor device.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, palette, section};
use uuid::Uuid;

use crate::dynamics::{
    History, HistorySample, METER_MIN_DB, ThresholdCurve, follow_peak, linear_to_db,
};
use crate::ui::knob_cell;
use crate::util::{Smoothed, db_to_gain, gain_to_db, time_coeff};

/// Device type id for the stock compressor.
pub const TYPE_ID: &str = "notwig.ext.devices.compressor";

const THRESHOLD_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0001;
const RATIO_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0002;
const ATTACK_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0003;
const RELEASE_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0004;
const KNEE_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0005;
const MAKEUP_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0006;
const MIX_UUID: u128 = 0x434f_4d50_0000_0000_0000_0000_0000_0007;

const DEFAULT_THRESHOLD_DB: f64 = -18.0;
const DEFAULT_RATIO: f64 = 4.0;
const DEFAULT_ATTACK_SECONDS: f64 = 0.01;
const DEFAULT_RELEASE_SECONDS: f64 = 0.15;
const DEFAULT_KNEE_DB: f64 = 6.0;
const DEFAULT_MAKEUP_DB: f64 = 0.0;
const DEFAULT_MIX: f64 = 1.0;

const MIN_THRESHOLD_DB: f64 = -60.0;
const MAX_THRESHOLD_DB: f64 = 0.0;
const MIN_RATIO: f64 = 1.0;
const MAX_RATIO: f64 = 20.0;
const MIN_ATTACK_SECONDS: f64 = 0.0001;
const MAX_ATTACK_SECONDS: f64 = 0.2;
const MIN_RELEASE_SECONDS: f64 = 0.01;
const MAX_RELEASE_SECONDS: f64 = 2.0;
const MIN_KNEE_DB: f64 = 0.0;
const MAX_KNEE_DB: f64 = 24.0;
const MIN_MAKEUP_DB: f64 = -24.0;
const MAX_MAKEUP_DB: f64 = 24.0;

/// Returns the compressor threshold parameter id.
#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

/// Returns the compressor ratio parameter id.
#[must_use]
pub fn ratio_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RATIO_UUID))
}

/// Returns the compressor attack parameter id.
#[must_use]
pub fn attack_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_UUID))
}

/// Returns the compressor release parameter id.
#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
}

/// Returns the compressor knee parameter id.
#[must_use]
pub fn knee_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(KNEE_UUID))
}

/// Returns the compressor make-up gain parameter id.
#[must_use]
pub fn makeup_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MAKEUP_UUID))
}

/// Returns the compressor dry/wet parameter id.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn threshold_norm(value: f64) -> f32 {
    ((value - MIN_THRESHOLD_DB) / (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)) as f32
}

fn ratio_norm(value: f64) -> f32 {
    ((value - MIN_RATIO) / (MAX_RATIO - MIN_RATIO)) as f32
}

fn attack_norm(value: f64) -> f32 {
    ((value - MIN_ATTACK_SECONDS) / (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)) as f32
}

fn release_norm(value: f64) -> f32 {
    ((value - MIN_RELEASE_SECONDS) / (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)) as f32
}

fn knee_norm(value: f64) -> f32 {
    ((value - MIN_KNEE_DB) / (MAX_KNEE_DB - MIN_KNEE_DB)) as f32
}

fn makeup_norm(value: f64) -> f32 {
    ((value - MIN_MAKEUP_DB) / (MAX_MAKEUP_DB - MIN_MAKEUP_DB)) as f32
}

fn mix_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn threshold_db(norm: f32) -> f64 {
    MIN_THRESHOLD_DB + f64::from(norm) * (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB)
}

fn ratio_value(norm: f32) -> f64 {
    MIN_RATIO + f64::from(norm) * (MAX_RATIO - MIN_RATIO)
}

fn attack_seconds(norm: f32) -> f64 {
    MIN_ATTACK_SECONDS + f64::from(norm) * (MAX_ATTACK_SECONDS - MIN_ATTACK_SECONDS)
}

fn release_seconds(norm: f32) -> f64 {
    MIN_RELEASE_SECONDS + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS)
}

fn knee_db(norm: f32) -> f64 {
    MIN_KNEE_DB + f64::from(norm) * (MAX_KNEE_DB - MIN_KNEE_DB)
}

fn makeup_db(norm: f32) -> f64 {
    MIN_MAKEUP_DB + f64::from(norm) * (MAX_MAKEUP_DB - MIN_MAKEUP_DB)
}

fn gain_reduction_db(input_db: f32, threshold_db: f64, ratio: f64, knee_db: f64) -> f32 {
    let threshold = threshold_db as f32;
    let ratio = ratio as f32;
    let knee = knee_db as f32;
    let slope = 1.0 - 1.0 / ratio;
    if knee > 0.0 {
        let lower = threshold - knee * 0.5;
        let upper = threshold + knee * 0.5;
        if input_db <= lower {
            0.0
        } else if input_db >= upper {
            (threshold - input_db) * slope
        } else {
            let x = input_db - lower;
            -(x * x / (2.0 * knee) * slope)
        }
    } else if input_db > threshold {
        (threshold - input_db) * slope
    } else {
        0.0
    }
}

/// Stereo-linked compressor with a soft knee.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct CompressorDevice {
    sample_rate: f64,
    channels: usize,
    threshold_db: f64,
    ratio: f64,
    attack_seconds: f64,
    release_seconds: f64,
    knee_db: f64,
    makeup_db: f64,
    mix: f64,
    attack_coeff: f32,
    release_coeff: f32,
    gain: f32,
    makeup: Smoothed,
    wet_mix: Smoothed,
    history: History,
    meter_left_db: f32,
    meter_right_db: f32,
    meter_peak_db: f32,
    level_db: f32,
    gain_reduction_db: f32,
    last_meter_time: f64,
    last_history_time: f64,
}

impl Default for CompressorDevice {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            channels: 2,
            threshold_db: DEFAULT_THRESHOLD_DB,
            ratio: DEFAULT_RATIO,
            attack_seconds: DEFAULT_ATTACK_SECONDS,
            release_seconds: DEFAULT_RELEASE_SECONDS,
            knee_db: DEFAULT_KNEE_DB,
            makeup_db: DEFAULT_MAKEUP_DB,
            mix: DEFAULT_MIX,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            gain: 1.0,
            makeup: Smoothed::new(db_to_gain(DEFAULT_MAKEUP_DB), 15.0, sample_rate),
            wet_mix: Smoothed::new(DEFAULT_MIX as f32, 15.0, sample_rate),
            history: History::new(),
            meter_left_db: METER_MIN_DB,
            meter_right_db: METER_MIN_DB,
            meter_peak_db: METER_MIN_DB,
            level_db: METER_MIN_DB,
            gain_reduction_db: 0.0,
            last_meter_time: 0.0,
            last_history_time: -1.0,
        }
    }
}

impl CompressorDevice {
    fn update_timing(&mut self) {
        self.attack_coeff = time_coeff(self.attack_seconds, self.sample_rate);
        self.release_coeff = time_coeff(self.release_seconds, self.sample_rate);
    }

    fn update_makeup(&mut self) {
        self.makeup.set_target(db_to_gain(self.makeup_db));
    }

    fn update_mix(&mut self) {
        self.wet_mix.set_target(self.mix as f32);
    }

    fn reset_meters(&mut self) {
        self.history = History::new();
        self.meter_left_db = METER_MIN_DB;
        self.meter_right_db = METER_MIN_DB;
        self.meter_peak_db = METER_MIN_DB;
        self.level_db = METER_MIN_DB;
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
        if self.last_history_time < 0.0 || now - self.last_history_time >= 1.0 / 60.0 {
            self.history.push(HistorySample { level_db, gain_db });
            self.last_history_time = now;
        }
    }
}

impl DeviceExtension for CompressorDevice {
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
                    ratio_id(),
                    "Ratio",
                    Unit::None,
                    MIN_RATIO,
                    MAX_RATIO,
                    DEFAULT_RATIO,
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
                    release_id(),
                    "Release",
                    Unit::Seconds,
                    MIN_RELEASE_SECONDS,
                    MAX_RELEASE_SECONDS,
                    DEFAULT_RELEASE_SECONDS,
                ),
                ParamSpec::continuous(
                    knee_id(),
                    "Knee",
                    Unit::Decibels,
                    MIN_KNEE_DB,
                    MAX_KNEE_DB,
                    DEFAULT_KNEE_DB,
                ),
                ParamSpec::continuous(
                    makeup_id(),
                    "Makeup",
                    Unit::Decibels,
                    MIN_MAKEUP_DB,
                    MAX_MAKEUP_DB,
                    DEFAULT_MAKEUP_DB,
                ),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Compressor".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(360.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        let now = ui.input(|input| input.time);
        self.update_meters(meter, now);
        section(ui, "Compressor");

        let width = ui.available_width();
        let meter_width = 38.0;
        let knob_height = 44.0;
        let label_height = 16.0;
        let graph_height =
            (ui.available_height() - (2.0 * knob_height + label_height + 12.0)).clamp(30.0, 76.0);
        ui.horizontal(|ui| {
            let graph_width = (width - meter_width - 6.0).max(60.0);
            let (graph_rect, _) =
                ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
            crate::dynamics::paint_history_graph(
                ui.painter(),
                graph_rect,
                &self.history,
                self.threshold_db as f32,
                ThresholdCurve::FollowGain,
                true,
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
        let gap = 4.0;
        let columns = 4.0;
        let knob_width = ((width - gap * (columns - 1.0)) / columns).clamp(40.0, 76.0);
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "threshold")),
                "Threshold",
                threshold_norm(self.threshold_db),
                threshold_norm(DEFAULT_THRESHOLD_DB),
                false,
                knob_width,
                knob_height,
            ) {
                self.threshold_db = threshold_db(norm);
                panel_frame::set_param_normalized(ui, threshold_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "ratio")),
                "Ratio",
                ratio_norm(self.ratio),
                ratio_norm(DEFAULT_RATIO),
                false,
                knob_width,
                knob_height,
            ) {
                self.ratio = ratio_value(norm);
                panel_frame::set_param_normalized(ui, ratio_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "attack")),
                "Attack",
                attack_norm(self.attack_seconds),
                attack_norm(DEFAULT_ATTACK_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.attack_seconds = attack_seconds(norm);
                self.update_timing();
                panel_frame::set_param_normalized(ui, attack_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "release")),
                "Release",
                release_norm(self.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.release_seconds = release_seconds(norm);
                self.update_timing();
                panel_frame::set_param_normalized(ui, release_id(), norm);
            }
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "knee")),
                "Knee",
                knee_norm(self.knee_db),
                knee_norm(DEFAULT_KNEE_DB),
                false,
                knob_width,
                knob_height,
            ) {
                self.knee_db = knee_db(norm);
                panel_frame::set_param_normalized(ui, knee_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "makeup")),
                "Makeup",
                makeup_norm(self.makeup_db),
                makeup_norm(DEFAULT_MAKEUP_DB),
                true,
                knob_width,
                knob_height,
            ) {
                self.makeup_db = makeup_db(norm);
                self.update_makeup();
                panel_frame::set_param_normalized(ui, makeup_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-compressor", "mix")),
                "Mix",
                mix_norm(self.mix),
                mix_norm(DEFAULT_MIX),
                false,
                knob_width,
                knob_height,
            ) {
                self.mix = f64::from(norm).clamp(0.0, 1.0);
                self.update_mix();
                panel_frame::set_param_normalized(ui, mix_id(), norm);
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.channels = usize::from(channels.max(1));
        self.gain = 1.0;
        self.update_timing();
        self.makeup.set_sample_rate(self.sample_rate, 15.0);
        self.makeup.snap();
        self.wet_mix.set_sample_rate(self.sample_rate, 15.0);
        self.wet_mix.snap();
        self.reset_meters();
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == threshold_id() {
            self.threshold_db = value.clamp(MIN_THRESHOLD_DB, MAX_THRESHOLD_DB);
        } else if param_id == ratio_id() {
            self.ratio = value.clamp(MIN_RATIO, MAX_RATIO);
        } else if param_id == attack_id() {
            self.attack_seconds = value.clamp(MIN_ATTACK_SECONDS, MAX_ATTACK_SECONDS);
            self.update_timing();
        } else if param_id == release_id() {
            self.release_seconds = value.clamp(MIN_RELEASE_SECONDS, MAX_RELEASE_SECONDS);
            self.update_timing();
        } else if param_id == knee_id() {
            self.knee_db = value.clamp(MIN_KNEE_DB, MAX_KNEE_DB);
        } else if param_id == makeup_id() {
            self.makeup_db = value.clamp(MIN_MAKEUP_DB, MAX_MAKEUP_DB);
            self.update_makeup();
        } else if param_id == mix_id() {
            self.mix = value.clamp(0.0, 1.0);
            self.update_mix();
        } else {
            return false;
        }
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        let channels = self.channels.max(1);
        for frame in samples.chunks_mut(channels) {
            let peak = frame
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let input_db = gain_to_db(peak);
            let reduction =
                gain_reduction_db(input_db, self.threshold_db, self.ratio, self.knee_db);
            let target_gain = db_to_gain(f64::from(reduction));
            let coeff = if target_gain < self.gain {
                self.attack_coeff
            } else {
                self.release_coeff
            };
            self.gain += (target_gain - self.gain) * coeff;
            if (target_gain - self.gain).abs() < 0.000_5 {
                self.gain = target_gain;
            }
            let makeup = self.makeup.next();
            let mix = self.wet_mix.next();
            let wet_gain = self.gain * makeup;
            for sample in frame {
                let dry = *sample;
                *sample = dry * (1.0 - mix) + dry * wet_gain * mix;
            }
        }
    }
}
