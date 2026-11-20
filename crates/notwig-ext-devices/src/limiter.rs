use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit};
use notwig_ext_api::{
    Color, DeviceExtension, DrawCommand, PanelCommand, PanelDraw, Point, Rect, Size,
};
use uuid::Uuid;

pub const TYPE_ID: &str = "notwig.ext.devices.limiter";

const THRESHOLD_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0001;
const RELEASE_UUID: u128 = 0x4c49_4d49_5445_5200_0000_0000_0000_0002;

const DEFAULT_THRESHOLD_DB: f64 = -6.0;
const DEFAULT_RELEASE_SECONDS: f64 = 0.1;

#[must_use]
pub fn threshold_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(THRESHOLD_UUID))
}

#[must_use]
pub fn release_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(RELEASE_UUID))
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
                    -24.0,
                    0.0,
                    -6.0,
                ),
                ParamSpec::continuous(release_id(), "Release", Unit::Seconds, 0.01, 0.5, 0.1),
            ],
            latency_samples: 0,
            name: "Limiter".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(240.0)),
        }
    }

    fn panel() -> PanelDraw {
        PanelDraw {
            commands: vec![
                PanelCommand::Section {
                    rect: Rect::from_min_size(Point::new(0.0, 2.0), Size::new(220.0, 14.0)),
                    title: "Limiter".into(),
                },
                PanelCommand::Knob {
                    rect: Rect::from_min_size(Point::new(12.0, 24.0), Size::new(78.0, 84.0)),
                    param_id: threshold_id(),
                    label: "Threshold".into(),
                    bipolar: true,
                },
                PanelCommand::Knob {
                    rect: Rect::from_min_size(Point::new(108.0, 24.0), Size::new(78.0, 84.0)),
                    param_id: release_id(),
                    label: "Release".into(),
                    bipolar: false,
                },
                PanelCommand::Draw(DrawCommand::Line {
                    from: Point::new(10.0, 116.0),
                    to: Point::new(194.0, 116.0),
                    width: 1.0,
                    color: Color::rgb(44, 47, 56),
                }),
                PanelCommand::Label {
                    rect: Rect::from_min_size(Point::new(10.0, 122.0), Size::new(200.0, 16.0)),
                    text: "Simple peak limiter".into(),
                },
            ],
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
            self.threshold_db = value.clamp(-24.0, 0.0);
            self.update_threshold();
            true
        } else if param_id == release_id() {
            self.release_seconds = value.clamp(0.01, 0.5);
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
