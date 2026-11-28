//! Stereo delay with crossfeed, damping, and dry-input ducking.

use std::time::Duration;

use egui::{Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, section};
use uuid::Uuid;

use crate::ui::knob_cell;
use crate::util::{DelayLine, OnePole, Smoothed, flush_denormal, seconds_to_samples};

/// Device type id for the stock delay.
pub const TYPE_ID: &str = "notwig.ext.devices.delay";

const TIME_LEFT_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0001;
const TIME_RIGHT_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0002;
const FEEDBACK_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0003;
const CROSSFEED_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0004;
const DAMPING_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0005;
const DUCKING_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0006;
const WIDTH_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0007;
const MIX_UUID: u128 = 0x444c_4159_0000_0000_0000_0000_0000_0008;

const DEFAULT_TIME_LEFT_SECONDS: f64 = 0.35;
const DEFAULT_TIME_RIGHT_SECONDS: f64 = 0.5;
const DEFAULT_FEEDBACK: f64 = 0.35;
const DEFAULT_CROSSFEED: f64 = 0.0;
const DEFAULT_DAMPING: f64 = 0.3;
const DEFAULT_DUCKING: f64 = 0.0;
const DEFAULT_WIDTH: f64 = 1.0;
const DEFAULT_MIX: f64 = 0.25;

const MIN_TIME_SECONDS: f64 = 0.001;
const MAX_TIME_SECONDS: f64 = 2.0;

/// Returns the left delay-time parameter id.
#[must_use]
pub fn time_left_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(TIME_LEFT_UUID))
}

/// Returns the right delay-time parameter id.
#[must_use]
pub fn time_right_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(TIME_RIGHT_UUID))
}

/// Returns the feedback parameter id.
#[must_use]
pub fn feedback_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(FEEDBACK_UUID))
}

/// Returns the crossfeed parameter id.
#[must_use]
pub fn crossfeed_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(CROSSFEED_UUID))
}

/// Returns the damping parameter id.
#[must_use]
pub fn damping_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DAMPING_UUID))
}

/// Returns the ducking parameter id.
#[must_use]
pub fn ducking_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DUCKING_UUID))
}

/// Returns the stereo-width parameter id.
#[must_use]
pub fn width_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(WIDTH_UUID))
}

/// Returns the dry/wet parameter id.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn time_norm(value: f64) -> f32 {
    ((value - MIN_TIME_SECONDS) / (MAX_TIME_SECONDS - MIN_TIME_SECONDS)) as f32
}

fn unit_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn time_seconds(norm: f32) -> f64 {
    MIN_TIME_SECONDS + f64::from(norm) * (MAX_TIME_SECONDS - MIN_TIME_SECONDS)
}

/// Stereo delay device.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct DelayDevice {
    sample_rate: f64,
    time_left: f64,
    time_right: f64,
    feedback: f64,
    crossfeed: f64,
    damping: f64,
    ducking: f64,
    width: f64,
    mix: f64,
    left: DelayLine,
    right: DelayLine,
    damp_left: OnePole,
    damp_right: OnePole,
    duck_envelope: f32,
    feedback_smoother: Smoothed,
    mix_smoother: Smoothed,
    meter_left: f32,
    meter_right: f32,
    meter_peak: f32,
}

impl Default for DelayDevice {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            time_left: DEFAULT_TIME_LEFT_SECONDS,
            time_right: DEFAULT_TIME_RIGHT_SECONDS,
            feedback: DEFAULT_FEEDBACK,
            crossfeed: DEFAULT_CROSSFEED,
            damping: DEFAULT_DAMPING,
            ducking: DEFAULT_DUCKING,
            width: DEFAULT_WIDTH,
            mix: DEFAULT_MIX,
            left: DelayLine::new(),
            right: DelayLine::new(),
            damp_left: OnePole::new(0.02, sample_rate),
            damp_right: OnePole::new(0.02, sample_rate),
            duck_envelope: 0.0,
            feedback_smoother: Smoothed::new(DEFAULT_FEEDBACK as f32, 15.0, sample_rate),
            mix_smoother: Smoothed::new(DEFAULT_MIX as f32, 15.0, sample_rate),
            meter_left: 0.0,
            meter_right: 0.0,
            meter_peak: 0.0,
        }
    }
}

impl DelayDevice {
    fn update_damping(&mut self) {
        let seconds = 0.005 + self.damping * 0.2;
        self.damp_left.set_time(seconds, self.sample_rate);
        self.damp_right.set_time(seconds, self.sample_rate);
    }

    fn update_feedback(&mut self) {
        self.feedback_smoother
            .set_target(self.feedback.clamp(0.0, 0.95) as f32);
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

    fn delay_samples(&self, seconds: f64) -> f32 {
        let samples = seconds * self.sample_rate;
        (samples as f32).clamp(1.0, (self.left_len() - 1) as f32)
    }

    fn left_len(&self) -> usize {
        (seconds_to_samples(MAX_TIME_SECONDS, self.sample_rate) + 2).max(2)
    }
}

impl DeviceExtension for DelayDevice {
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
                    time_left_id(),
                    "Time L",
                    Unit::Seconds,
                    MIN_TIME_SECONDS,
                    MAX_TIME_SECONDS,
                    DEFAULT_TIME_LEFT_SECONDS,
                ),
                ParamSpec::continuous(
                    time_right_id(),
                    "Time R",
                    Unit::Seconds,
                    MIN_TIME_SECONDS,
                    MAX_TIME_SECONDS,
                    DEFAULT_TIME_RIGHT_SECONDS,
                ),
                ParamSpec::continuous(
                    feedback_id(),
                    "Feedback",
                    Unit::Percent,
                    0.0,
                    0.95,
                    DEFAULT_FEEDBACK,
                ),
                ParamSpec::continuous(
                    crossfeed_id(),
                    "Crossfeed",
                    Unit::Percent,
                    0.0,
                    1.0,
                    DEFAULT_CROSSFEED,
                ),
                ParamSpec::continuous(
                    damping_id(),
                    "Damping",
                    Unit::Percent,
                    0.0,
                    1.0,
                    DEFAULT_DAMPING,
                ),
                ParamSpec::continuous(
                    ducking_id(),
                    "Ducking",
                    Unit::Percent,
                    0.0,
                    1.0,
                    DEFAULT_DUCKING,
                ),
                ParamSpec::continuous(width_id(), "Width", Unit::Percent, 0.0, 1.0, DEFAULT_WIDTH),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Delay".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(380.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        self.update_meters(meter);
        section(ui, "Delay");

        let width = ui.available_width();
        let meter_height = 30.0;
        ui.horizontal(|ui| {
            let (meter_rect, _) =
                ui.allocate_exact_size(Vec2::new(width, meter_height), Sense::hover());
            draw_stereo_meter_with_peak(
                ui.painter(),
                meter_rect,
                self.meter_left,
                self.meter_right,
                Some(self.meter_peak),
            );
        });

        ui.add_space(4.0);
        let gap = 4.0;
        let columns = 4.0;
        let knob_width = ((width - gap * (columns - 1.0)) / columns).clamp(40.0, 72.0);
        let knob_height = 42.0;
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "time-l")),
                "Time L",
                time_norm(self.time_left),
                time_norm(DEFAULT_TIME_LEFT_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.time_left = time_seconds(norm);
                panel_frame::set_param_normalized(ui, time_left_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "time-r")),
                "Time R",
                time_norm(self.time_right),
                time_norm(DEFAULT_TIME_RIGHT_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.time_right = time_seconds(norm);
                panel_frame::set_param_normalized(ui, time_right_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "feedback")),
                "Feedback",
                unit_norm(self.feedback),
                unit_norm(DEFAULT_FEEDBACK),
                false,
                knob_width,
                knob_height,
            ) {
                self.feedback = f64::from(norm).clamp(0.0, 0.95);
                self.update_feedback();
                panel_frame::set_param_normalized(ui, feedback_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "crossfeed")),
                "Cross",
                unit_norm(self.crossfeed),
                unit_norm(DEFAULT_CROSSFEED),
                false,
                knob_width,
                knob_height,
            ) {
                self.crossfeed = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, crossfeed_id(), norm);
            }
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "damping")),
                "Damping",
                unit_norm(self.damping),
                unit_norm(DEFAULT_DAMPING),
                false,
                knob_width,
                knob_height,
            ) {
                self.damping = f64::from(norm).clamp(0.0, 1.0);
                self.update_damping();
                panel_frame::set_param_normalized(ui, damping_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "ducking")),
                "Ducking",
                unit_norm(self.ducking),
                unit_norm(DEFAULT_DUCKING),
                false,
                knob_width,
                knob_height,
            ) {
                self.ducking = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, ducking_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "width")),
                "Width",
                unit_norm(self.width),
                unit_norm(DEFAULT_WIDTH),
                false,
                knob_width,
                knob_height,
            ) {
                self.width = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, width_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-delay", "mix")),
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
        let max_samples = seconds_to_samples(MAX_TIME_SECONDS, self.sample_rate) + 2;
        self.left.resize(max_samples);
        self.left.reset();
        self.right.resize(max_samples);
        self.right.reset();
        self.damp_left = OnePole::new(0.02, self.sample_rate);
        self.damp_right = OnePole::new(0.02, self.sample_rate);
        self.update_damping();
        self.feedback_smoother
            .set_sample_rate(self.sample_rate, 15.0);
        self.feedback_smoother.snap();
        self.mix_smoother.set_sample_rate(self.sample_rate, 15.0);
        self.mix_smoother.snap();
        self.duck_envelope = 0.0;
        self.meter_left = 0.0;
        self.meter_right = 0.0;
        self.meter_peak = 0.0;
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == time_left_id() {
            self.time_left = value.clamp(MIN_TIME_SECONDS, MAX_TIME_SECONDS);
        } else if param_id == time_right_id() {
            self.time_right = value.clamp(MIN_TIME_SECONDS, MAX_TIME_SECONDS);
        } else if param_id == feedback_id() {
            self.feedback = value.clamp(0.0, 0.95);
            self.update_feedback();
        } else if param_id == crossfeed_id() {
            self.crossfeed = value.clamp(0.0, 1.0);
        } else if param_id == damping_id() {
            self.damping = value.clamp(0.0, 1.0);
            self.update_damping();
        } else if param_id == ducking_id() {
            self.ducking = value.clamp(0.0, 1.0);
        } else if param_id == width_id() {
            self.width = value.clamp(0.0, 1.0);
        } else if param_id == mix_id() {
            self.mix = value.clamp(0.0, 1.0);
            self.update_mix();
        } else {
            return false;
        }
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        let delay_left = self.delay_samples(self.time_left);
        let delay_right = self.delay_samples(self.time_right);
        let crossfeed = self.crossfeed as f32;
        let width = self.width as f32;
        for frame in samples.chunks_exact_mut(2) {
            let dry_left = frame[0];
            let dry_right = frame[1];
            let delayed_left = self.left.read(delay_left);
            let delayed_right = self.right.read(delay_right);
            let damped_left = self.damp_left.process(delayed_left);
            let damped_right = self.damp_right.process(delayed_right);
            let feedback = self.feedback_smoother.next();
            let cross = crossfeed.clamp(0.0, 1.0);
            let feedback_left = damped_left * (1.0 - cross) + damped_right * cross;
            let feedback_right = damped_right * (1.0 - cross) + damped_left * cross;
            self.left
                .push(flush_denormal(dry_left + feedback_left * feedback));
            self.right
                .push(flush_denormal(dry_right + feedback_right * feedback));

            let dry_peak = dry_left.abs().max(dry_right.abs());
            let coeff = if dry_peak > self.duck_envelope {
                0.02
            } else {
                0.002
            };
            self.duck_envelope += (dry_peak - self.duck_envelope) * coeff;
            let duck = 1.0 - self.ducking as f32 * self.duck_envelope.clamp(0.0, 1.0);

            let mid = f32::midpoint(delayed_left, delayed_right);
            let side = (delayed_left - delayed_right) * 0.5 * width;
            let wet_left = (mid + side) * duck;
            let wet_right = (mid - side) * duck;
            let mix = self.mix_smoother.next();
            frame[0] = dry_left + (wet_left - dry_left) * mix;
            frame[1] = dry_right + (wet_right - dry_right) * mix;
        }
    }
}
