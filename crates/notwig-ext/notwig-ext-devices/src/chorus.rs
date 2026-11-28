//! Stereo chorus/flanger device.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{dropdown, palette, section};
use uuid::Uuid;

use crate::ui::knob_cell;
use crate::util::{DelayLine, OnePole, Smoothed, flush_denormal, seconds_to_samples};

/// Device type id for the stock chorus.
pub const TYPE_ID: &str = "notwig.ext.devices.chorus";

const MODE_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0001;
const RATE_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0002;
const DEPTH_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0003;
const FEEDBACK_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0004;
const DELAY_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0005;
const WIDTH_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0006;
const MIX_UUID: u128 = 0x4348_4f52_0000_0000_0000_0000_0000_0007;

const DEFAULT_MODE: u32 = 0;
const DEFAULT_RATE_HZ: f64 = 0.5;
const DEFAULT_DEPTH: f64 = 0.5;
const DEFAULT_FEEDBACK: f64 = 0.2;
const DEFAULT_DELAY_MS: f64 = 12.0;
const DEFAULT_WIDTH: f64 = 0.7;
const DEFAULT_MIX: f64 = 0.5;

const MIN_RATE_HZ: f64 = 0.05;
const MAX_RATE_HZ: f64 = 8.0;
const MIN_FEEDBACK: f64 = -0.9;
const MAX_FEEDBACK: f64 = 0.9;
const MIN_DELAY_MS: f64 = 1.0;
const MAX_DELAY_MS: f64 = 30.0;
const MAX_BUFFER_SECONDS: f64 = 0.06;

const MODE_OPTIONS: [&str; 2] = ["Chorus", "Flanger"];

/// Returns the chorus mode parameter id.
#[must_use]
pub fn mode_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MODE_UUID))
}

/// Returns the chorus rate parameter id.
#[must_use]
pub fn rate_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RATE_UUID))
}

/// Returns the chorus depth parameter id.
#[must_use]
pub fn depth_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DEPTH_UUID))
}

/// Returns the chorus feedback parameter id.
#[must_use]
pub fn feedback_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(FEEDBACK_UUID))
}

/// Returns the chorus base-delay parameter id.
#[must_use]
pub fn delay_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DELAY_UUID))
}

/// Returns the chorus stereo-width parameter id.
#[must_use]
pub fn width_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(WIDTH_UUID))
}

/// Returns the chorus dry/wet parameter id.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn rate_norm(value: f64) -> f32 {
    ((value - MIN_RATE_HZ) / (MAX_RATE_HZ - MIN_RATE_HZ)) as f32
}

fn feedback_norm(value: f64) -> f32 {
    ((value - MIN_FEEDBACK) / (MAX_FEEDBACK - MIN_FEEDBACK)) as f32
}

fn delay_norm(value: f64) -> f32 {
    ((value - MIN_DELAY_MS) / (MAX_DELAY_MS - MIN_DELAY_MS)) as f32
}

fn unit_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn rate_hz(norm: f32) -> f64 {
    MIN_RATE_HZ + f64::from(norm) * (MAX_RATE_HZ - MIN_RATE_HZ)
}

fn feedback_value(norm: f32) -> f64 {
    MIN_FEEDBACK + f64::from(norm) * (MAX_FEEDBACK - MIN_FEEDBACK)
}

fn delay_ms(norm: f32) -> f64 {
    MIN_DELAY_MS + f64::from(norm) * (MAX_DELAY_MS - MIN_DELAY_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChorusMode {
    Chorus,
    Flanger,
}

impl ChorusMode {
    fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::Flanger
        } else {
            Self::Chorus
        }
    }

    fn lfo(self, phase: f32) -> f32 {
        match self {
            Self::Chorus => (phase * std::f32::consts::TAU).sin(),
            Self::Flanger => {
                let phase = phase.rem_euclid(1.0);
                4.0 * (phase - 0.5).abs() - 1.0
            }
        }
    }
}

/// Stereo chorus/flanger device.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct ChorusDevice {
    sample_rate: f64,
    mode: ChorusMode,
    rate_hz: f64,
    depth: f64,
    feedback: f64,
    delay_ms: f64,
    width: f64,
    mix: f64,
    phase: f32,
    left: DelayLine,
    right: DelayLine,
    damp_left: OnePole,
    damp_right: OnePole,
    mix_smoother: Smoothed,
    meter_left: f32,
    meter_right: f32,
    meter_peak: f32,
}

impl Default for ChorusDevice {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            mode: ChorusMode::Chorus,
            rate_hz: DEFAULT_RATE_HZ,
            depth: DEFAULT_DEPTH,
            feedback: DEFAULT_FEEDBACK,
            delay_ms: DEFAULT_DELAY_MS,
            width: DEFAULT_WIDTH,
            mix: DEFAULT_MIX,
            phase: 0.0,
            left: DelayLine::new(),
            right: DelayLine::new(),
            damp_left: OnePole::new(0.01, sample_rate),
            damp_right: OnePole::new(0.01, sample_rate),
            mix_smoother: Smoothed::new(DEFAULT_MIX as f32, 15.0, sample_rate),
            meter_left: 0.0,
            meter_right: 0.0,
            meter_peak: 0.0,
        }
    }
}

impl ChorusDevice {
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

    fn base_delay_samples(&self) -> f32 {
        (self.delay_ms * self.sample_rate / 1_000.0) as f32
    }

    fn depth_samples(&self) -> f32 {
        self.base_delay_samples() * 0.55 * self.depth as f32
    }
}

impl DeviceExtension for ChorusDevice {
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
                ParamSpec::enumeration(mode_id(), "Mode", 2, DEFAULT_MODE),
                ParamSpec::continuous(
                    rate_id(),
                    "Rate",
                    Unit::Hertz,
                    MIN_RATE_HZ,
                    MAX_RATE_HZ,
                    DEFAULT_RATE_HZ,
                ),
                ParamSpec::continuous(depth_id(), "Depth", Unit::Percent, 0.0, 1.0, DEFAULT_DEPTH),
                ParamSpec::continuous(
                    feedback_id(),
                    "Feedback",
                    Unit::None,
                    MIN_FEEDBACK,
                    MAX_FEEDBACK,
                    DEFAULT_FEEDBACK,
                ),
                ParamSpec::continuous(
                    delay_id(),
                    "Delay",
                    Unit::Seconds,
                    MIN_DELAY_MS / 1_000.0,
                    MAX_DELAY_MS / 1_000.0,
                    DEFAULT_DELAY_MS / 1_000.0,
                ),
                ParamSpec::continuous(width_id(), "Width", Unit::Percent, 0.0, 1.0, DEFAULT_WIDTH),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Chorus".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(360.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        self.update_meters(meter);
        section(ui, "Chorus");

        let width = ui.available_width();
        let selected = usize::from(self.mode == ChorusMode::Flanger);
        if let Some(index) = dropdown(
            ui,
            ui.make_persistent_id(("ext-chorus", "mode")),
            selected,
            &MODE_OPTIONS,
        ) {
            self.mode = ChorusMode::from_index(index as u32);
            panel_frame::set_param_value(
                ui,
                mode_id(),
                notwig_core::ParamValue::EnumIndex(index as u32),
            );
        }

        let graph_height = (ui.available_height() - 62.0).clamp(30.0, 56.0);
        let (graph_rect, _) =
            ui.allocate_exact_size(Vec2::new(width, graph_height), Sense::hover());
        paint_lfo(ui.painter(), graph_rect, self.phase, self.mode, self.width);

        ui.add_space(4.0);
        let gap = 4.0;
        let columns = 6.0;
        let knob_width = ((width - gap * (columns - 1.0)) / columns).clamp(38.0, 64.0);
        let knob_height = 42.0;
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-chorus", "rate")),
                "Rate",
                rate_norm(self.rate_hz),
                rate_norm(DEFAULT_RATE_HZ),
                false,
                knob_width,
                knob_height,
            ) {
                self.rate_hz = rate_hz(norm);
                panel_frame::set_param_normalized(ui, rate_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-chorus", "depth")),
                "Depth",
                unit_norm(self.depth),
                unit_norm(DEFAULT_DEPTH),
                false,
                knob_width,
                knob_height,
            ) {
                self.depth = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, depth_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-chorus", "feedback")),
                "Feedback",
                feedback_norm(self.feedback),
                feedback_norm(DEFAULT_FEEDBACK),
                true,
                knob_width,
                knob_height,
            ) {
                self.feedback = feedback_value(norm);
                panel_frame::set_param_normalized(ui, feedback_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-chorus", "delay")),
                "Delay",
                delay_norm(self.delay_ms),
                delay_norm(DEFAULT_DELAY_MS),
                false,
                knob_width,
                knob_height,
            ) {
                self.delay_ms = delay_ms(norm);
                panel_frame::set_param_normalized(ui, delay_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-chorus", "width")),
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
                ui.make_persistent_id(("ext-chorus", "mix")),
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
        let max_samples = seconds_to_samples(MAX_BUFFER_SECONDS, self.sample_rate) + 2;
        self.left.resize(max_samples);
        self.left.reset();
        self.right.resize(max_samples);
        self.right.reset();
        self.damp_left = OnePole::new(0.008, self.sample_rate);
        self.damp_right = OnePole::new(0.008, self.sample_rate);
        self.mix_smoother.set_sample_rate(self.sample_rate, 15.0);
        self.mix_smoother.snap();
        self.phase = 0.0;
        self.meter_left = 0.0;
        self.meter_right = 0.0;
        self.meter_peak = 0.0;
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == mode_id() {
            self.mode = ChorusMode::from_index(value.round().max(0.0) as u32);
        } else if param_id == rate_id() {
            self.rate_hz = value.clamp(MIN_RATE_HZ, MAX_RATE_HZ);
        } else if param_id == depth_id() {
            self.depth = value.clamp(0.0, 1.0);
        } else if param_id == feedback_id() {
            self.feedback = value.clamp(MIN_FEEDBACK, MAX_FEEDBACK);
        } else if param_id == delay_id() {
            self.delay_ms = (value * 1_000.0).clamp(MIN_DELAY_MS, MAX_DELAY_MS);
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
        let base = self.base_delay_samples();
        let depth = self.depth_samples();
        let max_delay = (self.left_len() - 1) as f32;
        let phase_step = (self.rate_hz / self.sample_rate) as f32;
        let width = self.width as f32;
        let feedback = self.feedback as f32;
        for frame in samples.chunks_exact_mut(2) {
            let lfo_left = self.mode.lfo(self.phase);
            let lfo_right = self.mode.lfo((self.phase + width * 0.5).rem_euclid(1.0));
            self.phase = (self.phase + phase_step).rem_euclid(1.0);
            let read_left = (base + lfo_left * depth).clamp(0.0, max_delay);
            let read_right = (base + lfo_right * depth).clamp(0.0, max_delay);
            let delayed_left = self.left.read(read_left);
            let delayed_right = self.right.read(read_right);
            let damped_left = self.damp_left.process(delayed_left);
            let damped_right = self.damp_right.process(delayed_right);
            self.left
                .push(flush_denormal(frame[0] + damped_left * feedback));
            self.right
                .push(flush_denormal(frame[1] + damped_right * feedback));
            let mix = self.mix_smoother.next();
            frame[0] = frame[0] + (delayed_left - frame[0]) * mix;
            frame[1] = frame[1] + (delayed_right - frame[1]) * mix;
        }
    }
}

impl ChorusDevice {
    fn left_len(&self) -> usize {
        (seconds_to_samples(MAX_BUFFER_SECONDS, self.sample_rate) + 2).max(2)
    }
}

fn paint_lfo(painter: &egui::Painter, rect: egui::Rect, phase: f32, mode: ChorusMode, width: f64) {
    painter.rect_filled(rect, 3.0, palette::PANEL);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, palette::GRID),
        egui::StrokeKind::Inside,
    );
    let graph = rect.shrink(5.0);
    for channel in 0..2 {
        let offset = if channel == 0 {
            0.0
        } else {
            width as f32 * 0.5
        };
        let points: Vec<egui::Pos2> = (0..=64)
            .map(|index| {
                let t = index as f32 / 64.0;
                let value = mode.lfo((t + offset).rem_euclid(1.0));
                egui::pos2(
                    graph.left() + t * graph.width(),
                    graph.center().y - value * graph.height() * 0.4,
                )
            })
            .collect();
        let color = if channel == 0 {
            palette::ACCENT
        } else {
            palette::PLAYHEAD
        };
        painter.add(egui::Shape::line(points, Stroke::new(1.25_f32, color)));
    }
    let cursor_x = graph.left() + phase.rem_euclid(1.0) * graph.width();
    painter.line_segment(
        [
            egui::pos2(cursor_x, graph.min.y),
            egui::pos2(cursor_x, graph.max.y),
        ],
        Stroke::new(1.0_f32, palette::TEXT_MUTED),
    );
    painter.with_clip_rect(rect).text(
        rect.min + egui::vec2(6.0, 5.0),
        Align2::LEFT_TOP,
        "LFO",
        FontId::monospace(9.0),
        palette::TEXT_MUTED,
    );
}
