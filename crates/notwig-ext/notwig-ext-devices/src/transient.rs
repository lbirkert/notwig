//! Transient shaper device.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{dropdown, palette, section};
use uuid::Uuid;

use crate::ui::knob_cell;
use crate::util::{Smoothed, db_to_gain, time_coeff};

/// Device type id for the stock transient control.
pub const TYPE_ID: &str = "notwig.ext.devices.transient";

const ATTACK_GAIN_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0001;
const ATTACK_TIME_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0002;
const SUSTAIN_GAIN_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0003;
const SUSTAIN_TIME_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0004;
const MODE_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0005;
const MIX_UUID: u128 = 0x5452_414e_0000_0000_0000_0000_0000_0006;

const DEFAULT_ATTACK_GAIN_DB: f64 = 0.0;
const DEFAULT_ATTACK_TIME_SECONDS: f64 = 0.005;
const DEFAULT_SUSTAIN_GAIN_DB: f64 = 0.0;
const DEFAULT_SUSTAIN_TIME_SECONDS: f64 = 0.08;
const DEFAULT_MODE: u32 = 0;
const DEFAULT_MIX: f64 = 1.0;

const MIN_GAIN_DB: f64 = -24.0;
const MAX_GAIN_DB: f64 = 24.0;
const MIN_ATTACK_TIME_SECONDS: f64 = 0.000_5;
const MAX_ATTACK_TIME_SECONDS: f64 = 0.05;
const MIN_SUSTAIN_TIME_SECONDS: f64 = 0.01;
const MAX_SUSTAIN_TIME_SECONDS: f64 = 0.5;

const MODE_OPTIONS: [&str; 2] = ["Peak", "RMS"];

/// Returns the attack gain parameter id.
#[must_use]
pub fn attack_gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_GAIN_UUID))
}

/// Returns the attack time parameter id.
#[must_use]
pub fn attack_time_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(ATTACK_TIME_UUID))
}

/// Returns the sustain gain parameter id.
#[must_use]
pub fn sustain_gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(SUSTAIN_GAIN_UUID))
}

/// Returns the sustain time parameter id.
#[must_use]
pub fn sustain_time_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(SUSTAIN_TIME_UUID))
}

/// Returns the detector mode parameter id.
#[must_use]
pub fn mode_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MODE_UUID))
}

/// Returns the dry/wet parameter id.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn gain_norm(value: f64) -> f32 {
    ((value - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB)) as f32
}

fn attack_time_norm(value: f64) -> f32 {
    ((value - MIN_ATTACK_TIME_SECONDS) / (MAX_ATTACK_TIME_SECONDS - MIN_ATTACK_TIME_SECONDS)) as f32
}

fn sustain_time_norm(value: f64) -> f32 {
    ((value - MIN_SUSTAIN_TIME_SECONDS) / (MAX_SUSTAIN_TIME_SECONDS - MIN_SUSTAIN_TIME_SECONDS))
        as f32
}

fn unit_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn gain_db(norm: f32) -> f64 {
    MIN_GAIN_DB + f64::from(norm) * (MAX_GAIN_DB - MIN_GAIN_DB)
}

fn attack_time_seconds(norm: f32) -> f64 {
    MIN_ATTACK_TIME_SECONDS + f64::from(norm) * (MAX_ATTACK_TIME_SECONDS - MIN_ATTACK_TIME_SECONDS)
}

fn sustain_time_seconds(norm: f32) -> f64 {
    MIN_SUSTAIN_TIME_SECONDS
        + f64::from(norm) * (MAX_SUSTAIN_TIME_SECONDS - MIN_SUSTAIN_TIME_SECONDS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectorMode {
    Peak,
    Rms,
}

impl DetectorMode {
    fn from_index(index: u32) -> Self {
        if index == 1 { Self::Rms } else { Self::Peak }
    }
}

/// Transient shaper device.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct TransientDevice {
    sample_rate: f64,
    attack_gain_db: f64,
    attack_time_seconds: f64,
    sustain_gain_db: f64,
    sustain_time_seconds: f64,
    mode: DetectorMode,
    mix: f64,
    attack_gain: f32,
    sustain_gain: f32,
    fast_coeff: f32,
    slow_coeff: f32,
    fast_envelope: f32,
    slow_envelope: f32,
    gain_smoother: Smoothed,
    mix_smoother: Smoothed,
    transient_amount: f32,
    meter_left: f32,
    meter_right: f32,
    meter_peak: f32,
}

impl Default for TransientDevice {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        let attack_gain = db_to_gain(DEFAULT_ATTACK_GAIN_DB);
        let sustain_gain = db_to_gain(DEFAULT_SUSTAIN_GAIN_DB);
        Self {
            sample_rate,
            attack_gain_db: DEFAULT_ATTACK_GAIN_DB,
            attack_time_seconds: DEFAULT_ATTACK_TIME_SECONDS,
            sustain_gain_db: DEFAULT_SUSTAIN_GAIN_DB,
            sustain_time_seconds: DEFAULT_SUSTAIN_TIME_SECONDS,
            mode: DetectorMode::Peak,
            mix: DEFAULT_MIX,
            attack_gain,
            sustain_gain,
            fast_coeff: 0.0,
            slow_coeff: 0.0,
            fast_envelope: 0.0,
            slow_envelope: 0.0,
            gain_smoother: Smoothed::new(1.0, 2.0, sample_rate),
            mix_smoother: Smoothed::new(DEFAULT_MIX as f32, 15.0, sample_rate),
            transient_amount: 0.0,
            meter_left: 0.0,
            meter_right: 0.0,
            meter_peak: 0.0,
        }
    }
}

impl TransientDevice {
    fn update_timing(&mut self) {
        self.fast_coeff = time_coeff(self.attack_time_seconds, self.sample_rate);
        self.slow_coeff = time_coeff(self.sustain_time_seconds, self.sample_rate);
    }

    fn update_gains(&mut self) {
        self.attack_gain = db_to_gain(self.attack_gain_db);
        self.sustain_gain = db_to_gain(self.sustain_gain_db);
    }

    fn update_mix(&mut self) {
        self.mix_smoother.set_target(self.mix as f32);
    }

    fn update_meters(&mut self, meter: DeviceMeterSnapshot) {
        self.meter_left = meter.output_peak_l;
        self.meter_right = meter.output_peak_r;
        let peak = self.meter_left.max(self.meter_right);
        if peak >= self.meter_peak {
            self.meter_peak = peak;
        } else {
            self.meter_peak = (self.meter_peak - 0.02).max(0.0);
        }
    }
}

impl DeviceExtension for TransientDevice {
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
                    attack_gain_id(),
                    "Attack Gain",
                    Unit::Decibels,
                    MIN_GAIN_DB,
                    MAX_GAIN_DB,
                    DEFAULT_ATTACK_GAIN_DB,
                ),
                ParamSpec::continuous(
                    attack_time_id(),
                    "Attack Time",
                    Unit::Seconds,
                    MIN_ATTACK_TIME_SECONDS,
                    MAX_ATTACK_TIME_SECONDS,
                    DEFAULT_ATTACK_TIME_SECONDS,
                ),
                ParamSpec::continuous(
                    sustain_gain_id(),
                    "Sustain Gain",
                    Unit::Decibels,
                    MIN_GAIN_DB,
                    MAX_GAIN_DB,
                    DEFAULT_SUSTAIN_GAIN_DB,
                ),
                ParamSpec::continuous(
                    sustain_time_id(),
                    "Sustain Time",
                    Unit::Seconds,
                    MIN_SUSTAIN_TIME_SECONDS,
                    MAX_SUSTAIN_TIME_SECONDS,
                    DEFAULT_SUSTAIN_TIME_SECONDS,
                ),
                ParamSpec::enumeration(mode_id(), "Mode", 2, DEFAULT_MODE),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Transient".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(360.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        self.update_meters(meter);
        section(ui, "Transient");

        let width = ui.available_width();
        let selected = usize::from(self.mode == DetectorMode::Rms);
        ui.horizontal(|ui| {
            let response = dropdown(
                ui,
                ui.make_persistent_id(("ext-transient", "mode")),
                selected,
                &MODE_OPTIONS,
            );
            if let Some(index) = response {
                self.mode = DetectorMode::from_index(index as u32);
                panel_frame::set_param_value(ui, mode_id(), ParamValue::EnumIndex(index as u32));
            }
            let (meter_rect, _) =
                ui.allocate_exact_size(Vec2::new((width - 110.0).max(20.0), 20.0), Sense::hover());
            notwig_ui::draw_stereo_meter_with_peak(
                ui.painter(),
                meter_rect,
                self.meter_left,
                self.meter_right,
                Some(self.meter_peak),
            );
        });

        let graph_height = (ui.available_height() - 62.0).clamp(28.0, 54.0);
        let (graph_rect, _) =
            ui.allocate_exact_size(Vec2::new(width, graph_height), Sense::hover());
        paint_envelope(
            ui.painter(),
            graph_rect,
            self.attack_time_seconds,
            self.sustain_time_seconds,
            self.transient_amount,
        );

        ui.add_space(4.0);
        let gap = 4.0;
        let columns = 5.0;
        let knob_width = ((width - gap * (columns - 1.0)) / columns).clamp(40.0, 70.0);
        let knob_height = 42.0;
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-transient", "attack-gain")),
                "Attack",
                gain_norm(self.attack_gain_db),
                gain_norm(DEFAULT_ATTACK_GAIN_DB),
                true,
                knob_width,
                knob_height,
            ) {
                self.attack_gain_db = gain_db(norm);
                self.update_gains();
                panel_frame::set_param_normalized(ui, attack_gain_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-transient", "attack-time")),
                "Atk Time",
                attack_time_norm(self.attack_time_seconds),
                attack_time_norm(DEFAULT_ATTACK_TIME_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.attack_time_seconds = attack_time_seconds(norm);
                self.update_timing();
                panel_frame::set_param_normalized(ui, attack_time_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-transient", "sustain-gain")),
                "Sustain",
                gain_norm(self.sustain_gain_db),
                gain_norm(DEFAULT_SUSTAIN_GAIN_DB),
                true,
                knob_width,
                knob_height,
            ) {
                self.sustain_gain_db = gain_db(norm);
                self.update_gains();
                panel_frame::set_param_normalized(ui, sustain_gain_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-transient", "sustain-time")),
                "Sus Time",
                sustain_time_norm(self.sustain_time_seconds),
                sustain_time_norm(DEFAULT_SUSTAIN_TIME_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.sustain_time_seconds = sustain_time_seconds(norm);
                self.update_timing();
                panel_frame::set_param_normalized(ui, sustain_time_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-transient", "mix")),
                "Mix",
                unit_norm(self.mix),
                unit_norm(DEFAULT_MIX),
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

    fn prepare(&mut self, sample_rate: u32, _channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.update_timing();
        self.update_gains();
        self.fast_envelope = 0.0;
        self.slow_envelope = 0.0;
        self.transient_amount = 0.0;
        self.gain_smoother.set_sample_rate(self.sample_rate, 2.0);
        self.gain_smoother.snap();
        self.mix_smoother.set_sample_rate(self.sample_rate, 15.0);
        self.mix_smoother.snap();
        self.meter_left = 0.0;
        self.meter_right = 0.0;
        self.meter_peak = 0.0;
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == attack_gain_id() {
            self.attack_gain_db = value.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            self.update_gains();
        } else if param_id == attack_time_id() {
            self.attack_time_seconds =
                value.clamp(MIN_ATTACK_TIME_SECONDS, MAX_ATTACK_TIME_SECONDS);
            self.update_timing();
        } else if param_id == sustain_gain_id() {
            self.sustain_gain_db = value.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            self.update_gains();
        } else if param_id == sustain_time_id() {
            self.sustain_time_seconds =
                value.clamp(MIN_SUSTAIN_TIME_SECONDS, MAX_SUSTAIN_TIME_SECONDS);
            self.update_timing();
        } else if param_id == mode_id() {
            self.mode = DetectorMode::from_index(value.round().max(0.0) as u32);
        } else if param_id == mix_id() {
            self.mix = value.clamp(0.0, 1.0);
            self.update_mix();
        } else {
            return false;
        }
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.chunks_exact_mut(2) {
            let detector = match self.mode {
                DetectorMode::Peak => frame[0].abs().max(frame[1].abs()),
                DetectorMode::Rms => f32::midpoint(frame[0] * frame[0], frame[1] * frame[1]).sqrt(),
            };
            self.fast_envelope += (detector - self.fast_envelope) * self.fast_coeff;
            self.slow_envelope += (detector - self.slow_envelope) * self.slow_coeff;
            self.fast_envelope = crate::util::flush_denormal(self.fast_envelope);
            self.slow_envelope = crate::util::flush_denormal(self.slow_envelope);
            let transient = ((self.fast_envelope - self.slow_envelope)
                / self.fast_envelope.max(1.0e-6))
            .clamp(0.0, 1.0);
            self.transient_amount = transient;
            let target_gain = self.attack_gain * transient + self.sustain_gain * (1.0 - transient);
            self.gain_smoother.set_target(target_gain);
            let gain = self.gain_smoother.next();
            let mix = self.mix_smoother.next();
            for sample in frame {
                let dry = *sample;
                *sample = dry + (dry * gain - dry) * mix;
            }
        }
    }
}

fn paint_envelope(
    painter: &egui::Painter,
    rect: egui::Rect,
    attack_time: f64,
    sustain_time: f64,
    transient: f32,
) {
    painter.rect_filled(rect, 3.0, palette::PANEL);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, palette::GRID),
        egui::StrokeKind::Inside,
    );
    let graph = rect.shrink(5.0);
    let attack = (attack_time * 20.0).clamp(0.02, 0.35) as f32;
    let sustain = (sustain_time * 2.0).clamp(0.15, 0.7) as f32;
    let points: Vec<egui::Pos2> = (0..=64)
        .map(|index| {
            let t = index as f32 / 64.0;
            let value = if t < attack {
                t / attack
            } else {
                (1.0 - (t - attack) / sustain).max(0.0)
            };
            egui::pos2(
                graph.left() + t * graph.width(),
                graph.bottom() - value * graph.height() * 0.85,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.5_f32, palette::ACCENT),
    ));
    let transient_x = graph.left() + attack.min(0.6) * graph.width();
    let transient_height = (transient.clamp(0.0, 1.0) * graph.height() * 0.85).max(2.0);
    painter.line_segment(
        [
            egui::pos2(transient_x, graph.bottom()),
            egui::pos2(transient_x, graph.bottom() - transient_height),
        ],
        Stroke::new(2.0_f32, palette::PLAYHEAD),
    );
    painter.with_clip_rect(rect).text(
        rect.min + egui::vec2(6.0, 5.0),
        Align2::LEFT_TOP,
        "Envelope",
        FontId::monospace(9.0),
        palette::TEXT_MUTED,
    );
}
