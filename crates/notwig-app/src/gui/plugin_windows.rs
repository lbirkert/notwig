use notwig_core::DeviceId;

use super::NotwigApp;

impl NotwigApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn open_plugin_window(&mut self, device_id: DeviceId) {
        let Some(descriptor) = self.workspace.plugin_descriptor(device_id) else {
            self.notify_error("Plugin is not installed or was not found.".into());
            return;
        };
        let Some(audio) = &self.audio else {
            self.notify_error("Audio engine is not running.".into());
            return;
        };
        let sample_rate = self.workspace.doc.meta.sample_rate.value();
        let state = self.workspace.plugin_state(device_id).cloned();
        let params = self
            .workspace
            .doc
            .devices
            .get(&device_id)
            .map(|device| {
                device
                    .param_values
                    .iter()
                    .filter_map(|(param_id, value)| value.as_f64().map(|value| (*param_id, value)))
                    .collect()
            })
            .unwrap_or_default();
        if let Err(err) = audio.open_editor(device_id, descriptor, sample_rate, state, params) {
            self.notify_error(format!("Could not open plugin window: {err}"));
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn open_plugin_window(&mut self, _device_id: DeviceId) {
        self.notify_error("Native plugin editors are not available in the browser.".into());
    }
}
