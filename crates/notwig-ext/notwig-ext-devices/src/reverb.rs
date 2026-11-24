//! Stereo algorithmic reverb with Room/Hall/Plate voicings.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::{
    ContainerRouting, ContainerSpec, DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir,
    SignalType, Unit,
};
use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, dropdown, palette, section};
use uuid::Uuid;

use crate::ui::knob_cell;

/// Device type id for the stock reverb.
pub const TYPE_ID: &str = "notwig.ext.devices.reverb";

const TYPE_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0006;
const SIZE_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0001;
const DAMPING_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0002;
const PREDELAY_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0003;
const WIDTH_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0004;
const MIX_UUID: u128 = 0x5245_5645_5242_0000_0000_0000_0000_0005;

const DEFAULT_TYPE: u32 = 1;
const DEFAULT_SIZE: f64 = 0.6;
const DEFAULT_DAMPING: f64 = 0.4;
const DEFAULT_PREDELAY_SECONDS: f64 = 0.02;
const DEFAULT_WIDTH: f64 = 1.0;
const DEFAULT_MIX: f64 = 0.3;

const MAX_PREDELAY_SECONDS: f64 = 0.2;
const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;

const TYPE_OPTIONS: [&str; 3] = ["Room", "Hall", "Plate"];

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const PRE_ALLPASS_TUNING: [usize; 4] = [142, 107, 379, 277];
const STEREO_SPREAD: usize = 23;
const FIXED_GAIN: f32 = 0.015;
const MAX_SCALE: f64 = 1.25;

/// Returns the reverb type parameter id.
#[must_use]
pub fn reverb_type_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(TYPE_UUID))
}

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

fn unit_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn predelay_norm(seconds: f64) -> f32 {
    (seconds / MAX_PREDELAY_SECONDS).clamp(0.0, 1.0) as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReverbType {
    Room,
    Hall,
    Plate,
}

impl ReverbType {
    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Room,
            2 => Self::Plate,
            _ => Self::Hall,
        }
    }

    fn index(self) -> u32 {
        match self {
            Self::Room => 0,
            Self::Hall => 1,
            Self::Plate => 2,
        }
    }

    fn scale(self) -> f64 {
        match self {
            Self::Room => 0.78,
            Self::Hall => 1.15,
            Self::Plate => 0.56,
        }
    }

    fn comb_feedback(self, size: f32) -> f32 {
        match self {
            Self::Room => size.mul_add(0.26, 0.70),
            Self::Hall => size.mul_add(0.28, 0.72),
            Self::Plate => size.mul_add(0.20, 0.78),
        }
        .clamp(0.0, 0.98)
    }

    fn damping(self, damping: f32) -> f32 {
        match self {
            Self::Room => (damping * 1.1 + 0.1).clamp(0.0, 0.99),
            Self::Hall => damping.clamp(0.0, 0.99),
            Self::Plate => (damping * 0.55).clamp(0.0, 0.95),
        }
    }

    fn allpass_feedback(self) -> f32 {
        match self {
            Self::Room => 0.48,
            Self::Hall => 0.62,
            Self::Plate => 0.72,
        }
    }

    fn wet_gain(self) -> f32 {
        match self {
            Self::Room => 2.6,
            Self::Hall => 3.0,
            Self::Plate => 3.3,
        }
    }
}

#[derive(Debug, Clone)]
struct Comb {
    buffer: Vec<f32>,
    length: usize,
    position: usize,
    filter_store: f32,
}

impl Comb {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            buffer: vec![0.0; capacity],
            length: capacity,
            position: 0,
            filter_store: 0.0,
        }
    }

    fn set_length(&mut self, length: usize) {
        self.length = length.clamp(1, self.buffer.len());
        self.position %= self.length;
        self.filter_store = 0.0;
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
        if self.position >= self.length {
            self.position = 0;
        }
        output
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buffer: Vec<f32>,
    length: usize,
    position: usize,
}

impl Allpass {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            buffer: vec![0.0; capacity],
            length: capacity,
            position: 0,
        }
    }

    fn set_length(&mut self, length: usize) {
        self.length = length.clamp(1, self.buffer.len());
        self.position %= self.length;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.position = 0;
    }

    fn process(&mut self, input: f32, feedback: f32) -> f32 {
        let buffered = self.buffer[self.position];
        let output = -input + buffered;
        self.buffer[self.position] = input + buffered * feedback;
        self.position += 1;
        if self.position >= self.length {
            self.position = 0;
        }
        output
    }
}

/// Stereo algorithmic reverb with a host-owned wet FX chain.
#[derive(Debug, Clone)]
pub struct ReverbDevice {
    sample_rate: f64,
    reverb_type: ReverbType,
    size: f64,
    damping: f64,
    predelay_seconds: f64,
    width: f64,
    mix: f64,
    combs_left: Vec<Comb>,
    combs_right: Vec<Comb>,
    allpass_left: Vec<Allpass>,
    allpass_right: Vec<Allpass>,
    pre_allpass_left: Vec<Allpass>,
    pre_allpass_right: Vec<Allpass>,
    predelay: Vec<f32>,
    predelay_position: usize,
    max_predelay_samples: usize,
}

impl Default for ReverbDevice {
    fn default() -> Self {
        let mut reverb = Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            reverb_type: ReverbType::from_index(DEFAULT_TYPE),
            size: DEFAULT_SIZE,
            damping: DEFAULT_DAMPING,
            predelay_seconds: DEFAULT_PREDELAY_SECONDS,
            width: DEFAULT_WIDTH,
            mix: DEFAULT_MIX,
            combs_left: Vec::new(),
            combs_right: Vec::new(),
            allpass_left: Vec::new(),
            allpass_right: Vec::new(),
            pre_allpass_left: Vec::new(),
            pre_allpass_right: Vec::new(),
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
            .map(|tuning| Comb::new(capacity(*tuning, self.sample_rate)))
            .collect();
        self.combs_right = COMB_TUNING
            .iter()
            .map(|tuning| Comb::new(capacity(*tuning, self.sample_rate) + STEREO_SPREAD))
            .collect();
        self.allpass_left = ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(capacity(*tuning, self.sample_rate)))
            .collect();
        self.allpass_right = ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(capacity(*tuning, self.sample_rate) + STEREO_SPREAD))
            .collect();
        self.pre_allpass_left = PRE_ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(capacity(*tuning, self.sample_rate)))
            .collect();
        self.pre_allpass_right = PRE_ALLPASS_TUNING
            .iter()
            .map(|tuning| Allpass::new(capacity(*tuning, self.sample_rate) + STEREO_SPREAD))
            .collect();
        self.max_predelay_samples = (self.sample_rate * MAX_PREDELAY_SECONDS).round() as usize + 1;
        self.max_predelay_samples = self.max_predelay_samples.max(2);
        self.predelay = vec![0.0; self.max_predelay_samples * 2];
        self.predelay_position = 0;
        self.apply_type_lengths();
        self.reset_buffers();
    }

    fn apply_type_lengths(&mut self) {
        let scale = self.reverb_type.scale();
        for (comb, tuning) in self.combs_left.iter_mut().zip(COMB_TUNING) {
            comb.set_length(scaled_length(tuning, self.sample_rate, scale));
        }
        for (comb, tuning) in self.combs_right.iter_mut().zip(COMB_TUNING) {
            comb.set_length(scaled_length(tuning, self.sample_rate, scale) + STEREO_SPREAD);
        }
        for (allpass, tuning) in self.allpass_left.iter_mut().zip(ALLPASS_TUNING) {
            allpass.set_length(scaled_length(tuning, self.sample_rate, scale));
        }
        for (allpass, tuning) in self.allpass_right.iter_mut().zip(ALLPASS_TUNING) {
            allpass.set_length(scaled_length(tuning, self.sample_rate, scale) + STEREO_SPREAD);
        }
        for (allpass, tuning) in self.pre_allpass_left.iter_mut().zip(PRE_ALLPASS_TUNING) {
            allpass.set_length(scaled_length(tuning, self.sample_rate, scale));
        }
        for (allpass, tuning) in self.pre_allpass_right.iter_mut().zip(PRE_ALLPASS_TUNING) {
            allpass.set_length(scaled_length(tuning, self.sample_rate, scale) + STEREO_SPREAD);
        }
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
        for allpass in &mut self.pre_allpass_left {
            allpass.reset();
        }
        for allpass in &mut self.pre_allpass_right {
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

fn scaled_length(tuning: usize, sample_rate: f64, scale: f64) -> usize {
    (tuning as f64 * sample_rate / 44_100.0 * scale)
        .round()
        .max(4.0) as usize
}

fn capacity(tuning: usize, sample_rate: f64) -> usize {
    scaled_length(tuning, sample_rate, MAX_SCALE) + 2
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
                ParamSpec::enumeration(reverb_type_id(), "Type", 3, DEFAULT_TYPE),
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

        let selected = self.reverb_type.index() as usize;
        if let Some(index) = dropdown(
            ui,
            ui.make_persistent_id(("ext-reverb", "type")),
            selected,
            &TYPE_OPTIONS,
        ) {
            self.reverb_type = ReverbType::from_index(index as u32);
            self.apply_type_lengths();
            self.reset_buffers();
            panel_frame::set_param_value(ui, reverb_type_id(), ParamValue::EnumIndex(index as u32));
        }

        let width = ui.available_width();
        let meter_width = 38.0;
        let knob_height = 50.0;
        let label_height = 16.0;
        let reserved = knob_height + label_height + 8.0;
        let graph_height = (ui.available_height() - reserved).clamp(28.0, 76.0);
        ui.horizontal(|ui| {
            let graph_width = (width - meter_width - 6.0).max(60.0);
            let (graph_rect, _) =
                ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
            paint_decay_graph(
                ui.painter(),
                graph_rect,
                self.size,
                self.damping,
                self.reverb_type,
            );

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
            let knob_width = ((ui.available_width() - gap * 4.0) / 5.0).clamp(42.0, 68.0);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-reverb", "size")),
                "Size",
                unit_norm(self.size),
                unit_norm(DEFAULT_SIZE),
                false,
                knob_width,
                knob_height,
            ) {
                self.size = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, size_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-reverb", "damping")),
                "Damping",
                unit_norm(self.damping),
                unit_norm(DEFAULT_DAMPING),
                false,
                knob_width,
                knob_height,
            ) {
                self.damping = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, damping_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-reverb", "predelay")),
                "Pre-delay",
                predelay_norm(self.predelay_seconds),
                predelay_norm(DEFAULT_PREDELAY_SECONDS),
                false,
                knob_width,
                knob_height,
            ) {
                self.predelay_seconds = f64::from(norm) * MAX_PREDELAY_SECONDS;
                panel_frame::set_param_normalized(ui, predelay_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-reverb", "width")),
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
                ui.make_persistent_id(("ext-reverb", "mix")),
                "Mix",
                unit_norm(self.mix),
                unit_norm(DEFAULT_MIX),
                false,
                knob_width,
                knob_height,
            ) {
                self.mix = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, mix_id(), norm);
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn prepare(&mut self, sample_rate: u32, _channels: u16) {
        self.configure(f64::from(sample_rate.max(1)));
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == reverb_type_id() {
            let next = ReverbType::from_index(value.round().max(0.0) as u32);
            if next != self.reverb_type {
                self.reverb_type = next;
                self.apply_type_lengths();
                self.reset_buffers();
            }
        } else if param_id == size_id() {
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
        let feedback = self.reverb_type.comb_feedback(self.size as f32);
        let damping = self.reverb_type.damping(self.damping as f32);
        let allpass_feedback = self.reverb_type.allpass_feedback();
        let wet_gain = self.reverb_type.wet_gain();
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
            let mut input_left = self.predelay[read * 2] * FIXED_GAIN;
            let mut input_right = self.predelay[read * 2 + 1] * FIXED_GAIN;
            self.predelay_position = (write + 1) % len;

            for allpass in &mut self.pre_allpass_left {
                input_left = allpass.process(input_left, allpass_feedback);
            }
            for allpass in &mut self.pre_allpass_right {
                input_right = allpass.process(input_right, allpass_feedback);
            }

            let mut left = 0.0_f32;
            for comb in &mut self.combs_left {
                left += comb.process(input_left, feedback, damping);
            }
            let mut right = 0.0_f32;
            for comb in &mut self.combs_right {
                right += comb.process(input_right, feedback, damping);
            }
            for allpass in &mut self.allpass_left {
                left = allpass.process(left, allpass_feedback);
            }
            for allpass in &mut self.allpass_right {
                right = allpass.process(right, allpass_feedback);
            }

            frame[0] = (left * wet1 + right * wet2) * wet_gain;
            frame[1] = (right * wet1 + left * wet2) * wet_gain;
        }
    }
}

fn paint_decay_graph(
    painter: &egui::Painter,
    rect: egui::Rect,
    size: f64,
    damping: f64,
    reverb_type: ReverbType,
) {
    painter.rect_filled(rect, 3.0, palette::PANEL);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, palette::GRID),
        egui::StrokeKind::Inside,
    );
    let decay = (0.25 + size * 0.74).clamp(0.1, 0.999) as f32;
    let damping = reverb_type.damping(damping as f32);
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
        format!("{} decay", TYPE_OPTIONS[reverb_type.index() as usize]),
        FontId::monospace(9.0),
        palette::TEXT_MUTED,
    );
}
