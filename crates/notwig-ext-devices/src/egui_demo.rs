use notwig_core::{DeviceKind, ParamId, ParamValue, SchemaVersion};
use notwig_dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Unit};
use notwig_ext_api::{DeviceExtension, UiContext, egui, egui_panel, label, section};
use uuid::Uuid;

pub const TYPE_ID: &str = "notwig.ext.devices.egui-gain";

const GAIN_UUID: u128 = 0x4547_5549_4741_494e_0000_0000_0000_0001;

#[must_use]
pub fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(GAIN_UUID))
}

#[derive(Debug, Clone)]
pub struct EguiGainDevice {
    gain_db: f64,
    gain: f32,
}

impl Default for EguiGainDevice {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            gain: 1.0,
        }
    }
}

impl DeviceExtension for EguiGainDevice {
    const SUPPORTS_EGUI: bool = true;

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
                -24.0,
                24.0,
                0.0,
            )],
            latency_samples: 0,
            name: "Egui Gain".into(),
            vendor: "Notwig".into(),
            ui: DeviceUiSpec::custom("notwig.example.egui-gain", Some(320.0)),
        }
    }

    fn draw(&mut self, ui: &mut UiContext<'_>) {
        section(ui, "EGUI GAIN");
        label(ui, "This device can also draw with the command API.");
    }

    fn draw_egui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Egui Gain");
        ui.label("This panel runs the real egui API inside the extension.");
        ui.add_space(6.0);
        let mut gain_db = self.gain_db;
        if ui
            .add(egui::Slider::new(&mut gain_db, -24.0..=24.0).text("Gain (dB)"))
            .changed()
        {
            egui_panel::set_param_value(ui, gain_id(), ParamValue::F64(gain_db));
        }
        ui.separator();
        ui.label(format!("Linear gain: {:.3}", self.gain));
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
