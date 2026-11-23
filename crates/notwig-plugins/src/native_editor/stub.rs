use crate::types::{PluginDescriptor, PluginError, PluginState};

pub fn prepare_window_host() {}

pub fn pump_window_host() {}

pub struct NativePluginWindow;

impl NativePluginWindow {
    pub fn open(
        descriptor: &PluginDescriptor,
        _sample_rate: u32,
        _monitor: Option<std::sync::Arc<notwig_core::dsp::AudioMonitor>>,
    ) -> Result<Self, PluginError> {
        Err(PluginError::BackendUnavailable(descriptor.format))
    }

    pub fn focus(&self) {}

    pub fn pump(&mut self) {}

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn take_playback_toggle(&mut self) -> bool {
        false
    }

    pub fn set_parameter(&mut self, _id: u32, _value: f64) {}

    pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
        Vec::new()
    }

    pub fn save_state(&mut self) -> Result<PluginState, PluginError> {
        Err(PluginError::Load(
            "native plugin editors are unavailable on this platform".into(),
        ))
    }

    pub fn load_state(&mut self, _state: &PluginState) -> Result<(), PluginError> {
        Err(PluginError::Load(
            "native plugin editors are unavailable on this platform".into(),
        ))
    }
}
