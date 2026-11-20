use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_ext_api::{DeviceExtension, UiContext, button, divider, knob, label, section};
use uuid::Uuid;

pub const TYPE_ID: &str = "notwig.ext.devices.limiter";

const THRESHOLD_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0001;
const RELEASE_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0002;

const DEFAULT_THRESHOLD_DB: f64 = -6.0;
const DEFAULT_RELEASE_SECONDS: f64 = 0.1;

const MIN_THRESHOLD_DB: f64 = -24.0;
const MAX_THRESHOLD_DB: f64 = 0.0;
const MIN_RELEASE_SECONDS: f64 = 0.01;
const MAX_RELEASE_SECONDS: f64 = 0.5;

#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

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

#[derive(Debug, Clone)]
pub struct LimiterDevice {
    sample_rate: f64,
    channels: usize,
    threshold_db: f64,
    threshold: f32,
    release_seconds: f64,
    release_coeff: f32,
    gain: f32,
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
            latency_samples: 0,
            name: "Limiter".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(240.0)),
        }
    }

    fn draw(&mut self, ui: &mut UiContext<'_>) {
        section(ui, "Limiter");

        let knob_size = ui.style().knob_size;
        let mut threshold_change = None;
        let mut release_change = None;
        ui.horizontal(|ui| {
            let threshold_rect = ui.allocate(knob_size);
            let threshold_ui_id = ui.id("threshold");
            threshold_change = knob(
                ui,
                threshold_ui_id,
                threshold_rect,
                threshold_norm(self.threshold_db),
                threshold_norm(DEFAULT_THRESHOLD_DB),
                true,
                "Threshold",
            );

            let release_rect = ui.allocate(knob_size);
            let release_ui_id = ui.id("release");
            release_change = knob(
                ui,
                release_ui_id,
                release_rect,
                release_norm(self.release_seconds),
                release_norm(DEFAULT_RELEASE_SECONDS),
                false,
                "Release",
            );
        });

        if let Some(norm) = threshold_change {
            self.threshold_db =
                MIN_THRESHOLD_DB + f64::from(norm) * (MAX_THRESHOLD_DB - MIN_THRESHOLD_DB);
            self.update_threshold();
            ui.set_param_normalized(threshold_id(), norm);
        }

        if let Some(norm) = release_change {
            self.release_seconds =
                MIN_RELEASE_SECONDS + f64::from(norm) * (MAX_RELEASE_SECONDS - MIN_RELEASE_SECONDS);
            self.update_release();
            ui.set_param_normalized(release_id(), norm);
        }

        divider(ui);
        label(ui, "Simple peak limiter");

        if button(ui, "Reset").clicked() {
            self.threshold_db = DEFAULT_THRESHOLD_DB;
            self.release_seconds = DEFAULT_RELEASE_SECONDS;
            self.update_threshold();
            self.update_release();
            ui.set_param_value(threshold_id(), ParamValue::F64(DEFAULT_THRESHOLD_DB));
            ui.set_param_value(release_id(), ParamValue::F64(DEFAULT_RELEASE_SECONDS));
        }
    }

    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = f64::from(sample_rate.max(1));
        self.channels = usize::from(channels.max(1));
        self.gain = 1.0;
        self.update_release();
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
        let threshold = self.threshold;
        let release_coeff = self.release_coeff;
        let mut gain = self.gain;
        for frame in samples.chunks_mut(channels) {
            let peak = frame
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let target = if peak > threshold && peak > 0.0 {
                threshold / peak
            } else {
                1.0
            };
            if target < gain {
                gain = target;
            } else {
                gain += (target - gain) * release_coeff;
            }
            for sample in frame {
                *sample *= gain;
            }
        }
        self.gain = gain;
    }

    fn latency(&self) -> u32 {
        0
    }
}
