//! Soft clipper device.

use std::time::Duration;

use egui::{Align2, FontId, Sense, Stroke, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{draw_stereo_meter_with_peak, palette, section};
use uuid::Uuid;

use crate::ui::knob_cell;
use crate::util::{Smoothed, db_to_gain, soft_clip};

/// Device type id for the stock clipper.
pub const TYPE_ID: &str = "notwig.ext.devices.clipper";

const DRIVE_UUID: u128 = 0x434c_4950_0000_0000_0000_0000_0000_0001;
const KNEE_UUID: u128 = 0x434c_4950_0000_0000_0000_0000_0000_0002;
const TILT_UUID: u128 = 0x434c_4950_0000_0000_0000_0000_0000_0003;
const OUTPUT_UUID: u128 = 0x434c_4950_0000_0000_0000_0000_0000_0004;
const MIX_UUID: u128 = 0x434c_4950_0000_0000_0000_0000_0000_0005;

const DEFAULT_DRIVE_DB: f64 = 6.0;
const DEFAULT_KNEE: f64 = 0.7;
const DEFAULT_TILT: f64 = 0.0;
const DEFAULT_OUTPUT_DB: f64 = 0.0;
const DEFAULT_MIX: f64 = 1.0;

const MIN_DRIVE_DB: f64 = 0.0;
const MAX_DRIVE_DB: f64 = 36.0;
const MIN_OUTPUT_DB: f64 = -24.0;
const MAX_OUTPUT_DB: f64 = 24.0;

/// Returns the clipper drive parameter id.
#[must_use]
pub fn drive_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(DRIVE_UUID))
}

/// Returns the clipper knee/softness parameter id.
#[must_use]
pub fn knee_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(KNEE_UUID))
}

/// Returns the clipper tilt parameter id.
#[must_use]
pub fn tilt_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(TILT_UUID))
}

/// Returns the clipper output gain parameter id.
#[must_use]
pub fn output_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(OUTPUT_UUID))
}

/// Returns the clipper dry/wet parameter id.
#[must_use]
pub fn mix_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(MIX_UUID))
}

fn drive_norm(value: f64) -> f32 {
    ((value - MIN_DRIVE_DB) / (MAX_DRIVE_DB - MIN_DRIVE_DB)) as f32
}

fn knee_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn tilt_norm(value: f64) -> f32 {
    f64::midpoint(value, 1.0).clamp(0.0, 1.0) as f32
}

fn output_norm(value: f64) -> f32 {
    ((value - MIN_OUTPUT_DB) / (MAX_OUTPUT_DB - MIN_OUTPUT_DB)) as f32
}

fn mix_norm(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn drive_db(norm: f32) -> f64 {
    MIN_DRIVE_DB + f64::from(norm) * (MAX_DRIVE_DB - MIN_DRIVE_DB)
}

fn tilt_value(norm: f32) -> f64 {
    f64::from(norm) * 2.0 - 1.0
}

fn output_db(norm: f32) -> f64 {
    MIN_OUTPUT_DB + f64::from(norm) * (MAX_OUTPUT_DB - MIN_OUTPUT_DB)
}

#[derive(Debug, Clone, Copy, Default)]
struct DcBlocker {
    previous_input: f32,
    previous_output: f32,
}

impl DcBlocker {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = input - self.previous_input + 0.995 * self.previous_output;
        self.previous_input = input;
        self.previous_output = output;
        crate::util::flush_denormal(output)
    }
}

/// Soft clipper with tilt tone control.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct ClipperDevice {
    drive: f64,
    knee: f64,
    tilt: f64,
    output_db: f64,
    mix: f64,
    drive_gain: f32,
    output_gain: f32,
    mix_gain: Smoothed,
    tilt_left: crate::util::OnePole,
    tilt_right: crate::util::OnePole,
    dc_left: DcBlocker,
    dc_right: DcBlocker,
    meter_left: f32,
    meter_right: f32,
    meter_peak: f32,
}

impl Default for ClipperDevice {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        let drive_gain = db_to_gain(DEFAULT_DRIVE_DB);
        let output_gain = db_to_gain(DEFAULT_OUTPUT_DB);
        Self {
            drive: DEFAULT_DRIVE_DB,
            knee: DEFAULT_KNEE,
            tilt: DEFAULT_TILT,
            output_db: DEFAULT_OUTPUT_DB,
            mix: DEFAULT_MIX,
            drive_gain,
            output_gain,
            mix_gain: Smoothed::new(DEFAULT_MIX as f32, 15.0, sample_rate),
            tilt_left: crate::util::OnePole::new(0.000_2, sample_rate),
            tilt_right: crate::util::OnePole::new(0.000_2, sample_rate),
            dc_left: DcBlocker::default(),
            dc_right: DcBlocker::default(),
            meter_left: 0.0,
            meter_right: 0.0,
            meter_peak: 0.0,
        }
    }
}

impl ClipperDevice {
    fn update_drive(&mut self) {
        self.drive_gain = db_to_gain(self.drive);
    }

    fn update_output(&mut self) {
        self.output_gain = db_to_gain(self.output_db);
    }

    fn update_mix(&mut self) {
        self.mix_gain.set_target(self.mix as f32);
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

impl DeviceExtension for ClipperDevice {
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
                    drive_id(),
                    "Drive",
                    Unit::Decibels,
                    MIN_DRIVE_DB,
                    MAX_DRIVE_DB,
                    DEFAULT_DRIVE_DB,
                ),
                ParamSpec::continuous(knee_id(), "Knee", Unit::Percent, 0.0, 1.0, DEFAULT_KNEE),
                ParamSpec::continuous(tilt_id(), "Tilt", Unit::None, -1.0, 1.0, DEFAULT_TILT),
                ParamSpec::continuous(
                    output_id(),
                    "Output",
                    Unit::Decibels,
                    MIN_OUTPUT_DB,
                    MAX_OUTPUT_DB,
                    DEFAULT_OUTPUT_DB,
                ),
                ParamSpec::continuous(mix_id(), "Mix", Unit::Percent, 0.0, 1.0, DEFAULT_MIX),
            ],
            latency_samples: 0,
            name: "Clipper".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(340.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        self.update_meters(meter);
        section(ui, "Clipper");

        let width = ui.available_width();
        let meter_width = 34.0;
        let graph_height = (ui.available_height() - 78.0).clamp(42.0, 84.0);
        ui.horizontal(|ui| {
            let graph_width = (width - meter_width - 6.0).max(60.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(graph_width, graph_height), Sense::hover());
            paint_transfer(
                ui.painter(),
                rect,
                self.drive_gain,
                self.knee,
                self.output_gain,
            );
            let (meter_rect, _) =
                ui.allocate_exact_size(Vec2::new(meter_width, graph_height), Sense::hover());
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
        let knob_width = ((width - gap * 4.0) / 5.0).clamp(42.0, 68.0);
        let knob_height = 54.0;
        ui.horizontal(|ui| {
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-clipper", "drive")),
                "Drive",
                drive_norm(self.drive),
                drive_norm(DEFAULT_DRIVE_DB),
                false,
                knob_width,
                knob_height,
            ) {
                self.drive = drive_db(norm);
                self.update_drive();
                panel_frame::set_param_normalized(ui, drive_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-clipper", "knee")),
                "Knee",
                knee_norm(self.knee),
                knee_norm(DEFAULT_KNEE),
                false,
                knob_width,
                knob_height,
            ) {
                self.knee = f64::from(norm).clamp(0.0, 1.0);
                panel_frame::set_param_normalized(ui, knee_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-clipper", "tilt")),
                "Tilt",
                tilt_norm(self.tilt),
                tilt_norm(DEFAULT_TILT),
                true,
                knob_width,
                knob_height,
            ) {
                self.tilt = tilt_value(norm);
                panel_frame::set_param_normalized(ui, tilt_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-clipper", "output")),
                "Output",
                output_norm(self.output_db),
                output_norm(DEFAULT_OUTPUT_DB),
                true,
                knob_width,
                knob_height,
            ) {
                self.output_db = output_db(norm);
                self.update_output();
                panel_frame::set_param_normalized(ui, output_id(), norm);
            }
            ui.add_space(gap);
            if let Some(norm) = knob_cell(
                ui,
                ui.make_persistent_id(("ext-clipper", "mix")),
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

    fn prepare(&mut self, sample_rate: u32, _channels: u16) {
        let sample_rate = f64::from(sample_rate.max(1));
        self.mix_gain.set_sample_rate(sample_rate, 15.0);
        self.mix_gain.snap();
        self.tilt_left = crate::util::OnePole::new(0.000_2, sample_rate);
        self.tilt_right = crate::util::OnePole::new(0.000_2, sample_rate);
        self.dc_left = DcBlocker::default();
        self.dc_right = DcBlocker::default();
        self.meter_left = 0.0;
        self.meter_right = 0.0;
        self.meter_peak = 0.0;
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id == drive_id() {
            self.drive = value.clamp(MIN_DRIVE_DB, MAX_DRIVE_DB);
            self.update_drive();
        } else if param_id == knee_id() {
            self.knee = value.clamp(0.0, 1.0);
        } else if param_id == tilt_id() {
            self.tilt = value.clamp(-1.0, 1.0);
        } else if param_id == output_id() {
            self.output_db = value.clamp(MIN_OUTPUT_DB, MAX_OUTPUT_DB);
            self.update_output();
        } else if param_id == mix_id() {
            self.mix = value.clamp(0.0, 1.0);
            self.update_mix();
        } else {
            return false;
        }
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        let tilt = self.tilt as f32;
        let low_gain = db_to_gain(f64::from(-tilt * 6.0));
        let high_gain = db_to_gain(f64::from(tilt * 6.0));
        for frame in samples.chunks_exact_mut(2) {
            let dry_l = frame[0];
            let dry_r = frame[1];
            let driven_l = soft_clip(dry_l * self.drive_gain, self.knee as f32);
            let driven_r = soft_clip(dry_r * self.drive_gain, self.knee as f32);
            let blocked_l = self.dc_left.process(driven_l);
            let blocked_r = self.dc_right.process(driven_r);
            let low_l = self.tilt_left.process(blocked_l);
            let low_r = self.tilt_right.process(blocked_r);
            let shaped_l = (low_l * low_gain + (blocked_l - low_l) * high_gain) * self.output_gain;
            let shaped_r = (low_r * low_gain + (blocked_r - low_r) * high_gain) * self.output_gain;
            let mix = self.mix_gain.next();
            frame[0] = dry_l + (shaped_l - dry_l) * mix;
            frame[1] = dry_r + (shaped_r - dry_r) * mix;
        }
    }
}

fn paint_transfer(painter: &egui::Painter, rect: egui::Rect, drive: f32, knee: f64, output: f32) {
    painter.rect_filled(rect, 3.0, palette::PANEL);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, palette::GRID),
        egui::StrokeKind::Inside,
    );
    let graph = rect.shrink(6.0);
    painter.line_segment(
        [
            egui::pos2(graph.center().x, graph.min.y),
            egui::pos2(graph.center().x, graph.max.y),
        ],
        Stroke::new(1.0_f32, palette::GRID),
    );
    painter.line_segment(
        [
            egui::pos2(graph.min.x, graph.center().y),
            egui::pos2(graph.max.x, graph.center().y),
        ],
        Stroke::new(1.0_f32, palette::GRID),
    );
    let steps = 64;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|index| {
            let t = index as f32 / steps as f32;
            let input = t * 2.4 - 1.2;
            let output_value = soft_clip(input * drive, knee as f32) * output;
            egui::pos2(
                graph.center().x + input / 1.2 * graph.width() * 0.5,
                graph.center().y - output_value.clamp(-1.2, 1.2) / 1.2 * graph.height() * 0.5,
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
        "Transfer",
        FontId::monospace(9.0),
        palette::TEXT_MUTED,
    );
}
