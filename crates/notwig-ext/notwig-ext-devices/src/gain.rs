use egui::{Align2, FontId, Sense, Vec2};
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Unit};
use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{knob, label, palette, section};
use uuid::Uuid;

/// Device type id for the stock gain device.
pub const TYPE_ID: &str = "notwig.ext.devices.gain";

const GAIN_UUID: u128 = 0x4547_5549_4741_494e_0000_0000_0000_0001;

const MIN_GAIN_DB: f64 = -24.0;
const MAX_GAIN_DB: f64 = 24.0;

/// Returns the gain parameter id.
#[must_use]
pub fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(GAIN_UUID))
}

/// Stock gain (trim) device.
#[derive(Debug, Clone, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
pub struct GainDevice {
    gain_db: f64,
    gain: f32,
}

impl Default for GainDevice {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            gain: 1.0,
        }
    }
}

fn gain_norm(db: f64) -> f32 {
    ((db - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB)) as f32
}

impl DeviceExtension for GainDevice {
    fn new() -> Self {
        Self::default()
    }

    fn spec() -> DeviceSpec {
        DeviceSpec {
            type_id: TYPE_ID.into(),
            version: SchemaVersion::V0,
            kind: DeviceKind::AudioFx,
            ports: Vec::new(),
            params: vec![ParamSpec::continuous(
                gain_id(),
                "Gain",
                Unit::Decibels,
                MIN_GAIN_DB,
                MAX_GAIN_DB,
                0.0,
            )],
            latency_samples: 0,
            name: "Gain".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(320.0)),
            container: None,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
        section(ui, "Gain");
        label(ui, "This panel uses the shared notwig-ui components.");
        ui.add_space(6.0);

        let width = ui.available_width().min(78.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 58.0), Sense::hover());
        if let Some(norm) = knob(
            ui,
            ui.make_persistent_id(("gain", "gain")),
            rect,
            gain_norm(self.gain_db),
            gain_norm(0.0),
            true,
            "Gain (dB)",
        ) {
            let gain_db = MIN_GAIN_DB + f64::from(norm) * (MAX_GAIN_DB - MIN_GAIN_DB);
            self.gain_db = gain_db;
            self.gain = 10.0_f32.powf((gain_db / 20.0) as f32);
            panel_frame::set_param_value(ui, gain_id(), ParamValue::F64(gain_db));
        }
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.center().x, rect.bottom() + 8.0),
            Align2::CENTER_TOP,
            format!("{:+.1} dB", self.gain_db),
            FontId::proportional(10.0),
            palette::TEXT_MUTED,
        );
        ui.add_space(16.0);
        label(ui, &format!("Linear gain: {:.3}", self.gain));
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id != gain_id() {
            return false;
        }
        self.gain_db = value;
        self.gain = 10.0_f32.powf((value / 20.0) as f32);
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            *sample *= self.gain;
        }
    }
}
