use std::time::Duration;

use egui::{Align2, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::{
    ContainerRouting, ContainerSpec, DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir,
    SignalType, Unit,
};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, knob, palette, section};
use uuid::Uuid;

/// Device type id for the stock reverb.
pub const TYPE_ID: &str = "notwig.ext.devices.reverb";

const SIZE_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0001;
const DAMPING_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0002;
const PREDELAY_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0003;
const WIDTH_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0004;
const MIX_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0005;

const DEFAULT_SIZE: f64 = 0.6;
const DEFAULT_DAMPING: f64 = 0.4;
const DEFAULT_PREDELAY_SECONDS: f64 = 0.02;
const DEFAULT_WIDTH: f64 = 1.0;
const DEFAULT_MIX: f64 = 0.3;

const MAX_PREDELAY_SECONDS: f64 = 0.2;
const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const FIXED_GAIN: f32 = 0.015;
const WET_GAIN: f32 = 3.0;

/// Returns the room-size parameter id.
#[must_use]
pub fn size_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(SIZE_UUID))
}

/// Returns the damping parameter id.
#[must_use]
pub fn damping_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DAMPING_UUID))
}

/// Returns the pre-delay parameter id.
#[must_use]
pub fn predelay_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(PREDELAY_UUID))
}

/// Returns the stereo-width parameter id.
#[must_use]
pub fn width_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(WIDTH_UUID))
}

/// Returns the dry/wet mix parameter id.
///
/// This is a container wet parameter: the host applies the dry/wet blend after
/// the reverb stage and its wet FX chain.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn size_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn damping_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn predelay_norm(seconds: f64) -> f32 {
    (seconds / MAX_PREDELAY_SECONDS).clamp(0.0, 1.0) as f32
}

fn width_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn mix_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn knob_label(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    let clip = rect.expand2(egui::vec2(0.0, 18.0));
    ui.painter().with_clip_rect(clip).text(
        egui::pos2(rect.center().x, rect.bottom() + 8.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(10.0),
        palette::TEXT_MUTED,
    );
}

fn draw_knob(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    value: f32,
    default: f32,
    width: f32,
    height: f32,
) -> Option<f32> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let changed = knob(
        ui,
        ui.make_persistent_id(("ext-reverb", id_salt)),
        rect,
        value,
        default,
        false,
        label,
    );
    knob_label(ui, rect, label);
    changed
}

#[derive(Debug, Clone)]
struct Comb {
    buffer: Vec<f32>,
    position: usize,
    filter_store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            position: 0,
            filter_store: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.position = 0;
        self.filter_store = 0.0;
    }

    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.position];
        self.filter_store = output * (1.0 - damping) + self.filter_store * damping;
        if self.filter_store.abs() < 1.0e-20 {
            self.filter_store = 0.0;
        }
        self.buffer[self.position] = input + self.filter_store * feedback;
        self.position += 1;
        if self.position >= self.buffer.len() {
            self.position = 0;
        }
        output
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buffer: Vec<f32>,
    position: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            position: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.position = 0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.position];
        let output = -input + buffered;
        self.buffer[self.position] = input + buffered * 0.5;
        self.position += 1;
        if self.position >= self.buffer.len() {
            self.position = 0;
        }
        output
    }
}

/// Stereo algorithmic reverb with a host-owned wet FX chain.
#[derive(Debug, Clone)]
pub struct ReverbDevice {
    sample_rate: f64,
    size: f64,
    damping: f64,
    predelay_seconds: f64,
    width: f64,
    mix: f64,
    combs_left: Vec<Comb>,
    combs_right: Vec<Comb>,
    allpass_left: Vec<Allpass>,
    allpass_right: Vec<Allpass>,
    predelay: Vec<f32>,
    predelay_position: usize,
    max_predelay_samples: usize,
}

impl Default for ReverbDevice {
    fn default() -> Self {
        let mut reverb = Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            size: DEFAULT_SIZE,
            damping: DEFAULT_DAMPING,
            predelay_seconds: DEFAULT_PREDELAY_SECONDS,
            width: DEFAULT_WIDTH,
            mix: DEFAULT_MIX,
            combs_left: Vec::new(),
            combs_right: Vec::new(),
            allpass_left: Vec::new(),
            allpass_right: Vec::new(),
            predelay: Vec::new(),
            predelay_position: 0,
            max_predelay_samples: 1,
        };
        reverb.configure(DEFAULT_SAMPLE_RATE);
        reverb
    }
}

impl ReverbDevice {
    fn configure(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.combs_left = COMB_TUNING
            .iter()
            .map(|tuning| Comb::new(scaled_length(*tuning, self.sample_rate)))
            .collect();
        self.combs_right = COMB_TUNING
            .iter()
            .map(|tuning| Comb::new(scaled_length(*tuning, self.sample_rate) + STEREO_SPREAD))
            .collect();
        self.allpass_left = ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(scaled_length(*tuning, self.sample_rate)))
            .collect();
        self.allpass_right = ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(scaled_length(*tuning, self.sample_rate) + STEREO_SPREAD))
            .collect();
        self.max_predelay_samples = (self.sample_rate * MAX_PREDELAY_SECONDS).round() as usize + 1;
        self.max_predelay_samples = self.max_predelay_samples.max(2);
        self.predelay = vec![0.0; self.max_predelay_samples * 2];
        self.predelay_position = 0;
    }

    fn reset_buffers(&mut self) {
        for comb in &mut self.combs_left {
            comb.reset();
        }
        for comb in &mut self.combs_right {
            comb.reset();
        }
        for allpass in &mut self.allpass_left {
            allpass.reset();
        }
        for allpass in &mut self.allpass_right {
            allpass.reset();
        }
        self.predelay.fill(0.0);
        self.predelay_position = 0;
    }

    fn predelay_samples(&self) -> usize {
        let samples = (self.predelay_seconds * self.sample_rate).round() as usize;
        samples.min(self.max_predelay_samples.saturating_sub(1))
    }
}

fn scaled_length(tuning: usize, sample_rate: f64) -> usize {
    (tuning as f64 * sample_rate / 44_100.0).round().max(4.0) as usize
}

impl DeviceExtension for ReverbDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        DeviceSpec {
            type_id: TYPE_ID.into(),
            version: SchemaVersion::V0,
            kind: DeviceKind::Container,
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
                ParamSpec::continuous(size_id(), "Size", Unit::Percent, 0.0, 1.0, DEFAULT_SIZE),
                ParamSpec::continuous(
                    damping_id(),
                    "Damping",
                    Unit::Percent,
                    0.0,
                    1.0,
                    DEFAULT_DAMPING,
                ),
                ParamSpec::continuous(
                    predelay_id(),
                    "Pre-delay",
                    Unit::Seconds,
                    0.0,
                    MAX_PREDELAY_SECONDS,
                    DEFAULT_PREDELAY_SECONDS,
                ),
                ParamSpec::continuous(width_id(), "Width", Unit::Percent, 0.0, 1.0, DEFAULT_WIDTH),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Reverb".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(360.0)),
            container: Some(ContainerSpec {
                routing: ContainerRouting::StageThenChildren,
                wet_param: Some(mix_id()),
            }),
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        section(ui, "Reverb");

        let width = ui.available_width();
        let meter_width = 38.0;
        let knob_height = 54.0;
        let label_height = 16.0;
        let reserved = knob_height + label_height + 8.0;
        let graph_height = (ui.available_height() - reserved).clamp(32.0, 92.0);
        ui.horizontal(|ui| {
            let graph_width = (width - meter_width - 6.0).max(60.0);
            let (graph_rect, _) =
                ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
            paint_decay_graph(ui.painter(), graph_rect, self.size, self.damping);

            let (meter_rect, _) =
                ui.allocate_exact_size(Vec2::new(meter_width, graph_height), Sense::hover());
            draw_stereo_meter_with_peak(
                ui.painter(),
                meter_rect,
                meter.output_peak_l,
                meter.output_peak_r,
                Some(meter.output_peak_l.max(meter.output_peak_r)),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let gap = 4.0;
            let knob_width = ((ui.available_width() - gap * 4.0) / 5.0).clamp(44.0, 68.0);
            if let Some(norm) = draw_knob(
                ui,
                "size",
                "Size",
                size_norm(self.size),
                size_norm(DEFAULT_SIZE),
                knob_width,
                knob_height,
            ) {
                self.size = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, size_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = draw_knob(
                ui,
                "damping",
                "Damping",
                damping_norm(self.damping),
                damping_norm(DEFAULT_DAMPING),
                knob_width,
                knob_height,
            ) {
                self.damping = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, damping_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = draw_knob(
                ui,
                "predelay",
                "Pre-delay",
                predelay_norm(self.predelay_seconds),
                predelay_norm(DEFAULT_PREDELAY_SECONDS),
                knob_width,
                knob_height,
            ) {
                self.predelay_seconds = f64::from(norm) * MAX_PREDELAY_SECONDS;
                panel_frame::set_param_normalized(ui, predelay_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = draw_knob(
                ui,
                "width",
                "Width",
                width_norm(self.width),
                width_norm(DEFAULT_WIDTH),
                knob_width,
                knob_height,
            ) {
                self.width = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, width_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = draw_knob(
                ui,
                "mix",
                "Mix",
                mix_norm(self.mix),
                mix_norm(DEFAULT_MIX),
                knob_width,
                knob_height,
            ) {
                self.mix = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, mix_id(), norm);
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }

    fn prepare(&mut self, sample_rate: u32, _channels: u16) {
        self.configure(f64::from(sample_rate.max(1)));
        self.reset_buffers();
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == size_id() {
            self.size = value.clamp(0.0, 1.0);
        } else if param_id == damping_id() {
            self.damping = value.clamp(0.0, 1.0);
        } else if param_id == predelay_id() {
            self.predelay_seconds = value.clamp(0.0, MAX_PREDELAY_SECONDS);
        } else if param_id == width_id() {
            self.width = value.clamp(0.0, 1.0);
        } else if param_id == mix_id() {
            self.mix = value.clamp(0.0, 1.0);
        } else {
            return false;
        }
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        if self.predelay.is_empty() || self.combs_left.is_empty() {
            samples.fill(0.0);
            return;
        }
        let feedback = (self.size as f32).mul_add(0.28, 0.7).clamp(0.0, 0.98);
        let damping = (self.damping as f32).clamp(0.0, 0.99);
        let width = self.width as f32;
        let wet1 = width.mul_add(0.5, 0.5);
        let wet2 = (1.0 - width) * 0.5;
        let predelay = self.predelay_samples();
        let len = self.predelay.len() / 2;

        for frame in samples.chunks_exact_mut(2) {
            let write = self.predelay_position;
            self.predelay[write * 2] = frame[0];
            self.predelay[write * 2 + 1] = frame[1];
            let read = (write + len - predelay) % len;
            let input_left = self.predelay[read * 2] * FIXED_GAIN;
            let input_right = self.predelay[read * 2 + 1] * FIXED_GAIN;
            self.predelay_position = (write + 1) % len;

            let mut left = 0.0_f32;
            for comb in &mut self.combs_left {
                left += comb.process(input_left, feedback, damping);
            }
            let mut right = 0.0_f32;
            for comb in &mut self.combs_right {
                right += comb.process(input_right, feedback, damping);
            }
            for allpass in &mut self.allpass_left {
                left = allpass.process(left);
            }
            for allpass in &mut self.allpass_right {
                right = allpass.process(right);
            }

            frame[0] = (left * wet1 + right * wet2) * WET_GAIN;
            frame[1] = (right * wet1 + left * wet2) * WET_GAIN;
        }
    }
}

fn paint_decay_graph(painter: &egui::Painter, rect: egui::Rect, size: f64, damping: f64) {
    painter.rect_filled(rect, 3.0, palette::PANEL);
    let decay = (0.25 + size * 0.74).clamp(0.1, 0.999) as f32;
    let damping = damping.clamp(0.0, 1.0) as f32;
    let steps = 72;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|index| {
            let t = index as f32 / steps as f32;
            let curve = (decay.powf(t * (1.0 + damping * 3.0))).clamp(0.0, 1.0);
            egui::pos2(
                rect.left() + t * rect.width(),
                rect.bottom() - curve * (rect.height() - 8.0) - 4.0,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.5_f32, palette::ACCENT),
    ));
    painter.with_clip_rect(rect).text(
        rect.min + egui::vec2(6.0, 5.0),
        Align2::LEFT_TOP,
        "Wet decay",
        FontId::monospace(9.0),
        palette::TEXT_MUTED,
    );
}
