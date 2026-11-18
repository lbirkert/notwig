mod browser;
mod eq;
mod params;

pub use browser::DeviceBrowserState;
pub use eq::EqDrag;

use notwig_core::DeviceInstance;

use crate::shell::NotwigUi;

impl NotwigUi {
    pub(crate) fn eq_card_body_from_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DeviceInstance,
        sample_rate: f64,
    ) {
        self.eq_card_body(ui, device, sample_rate);
    }

    pub(crate) fn param_editor_from_panel(&mut self, ui: &mut egui::Ui, device: &DeviceInstance) {
        self.param_editor(ui, device);
    }
}
