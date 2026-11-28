use std::sync::Arc;

use notwig_core::dsp::{AudioMonitor, MonitorBlock};
use notwig_rack_editor::EditorWindow;

use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginState};

/// Notwig-owned adapter around the format-agnostic rack editor window.
///
/// The rack crate owns the native window, `AppKit` lifecycle, editor sizing,
/// parameter polling, and state plumbing. This wrapper adds Notwig's
/// descriptor/state types and the engine monitor tap.
pub struct NativePluginWindow {
    inner: EditorWindow,
    format: PluginFormat,
    plugin_uid: String,
    plugin_version: String,
    monitor: Option<Arc<AudioMonitor>>,
    monitor_block: Vec<f32>,
}

impl NativePluginWindow {
    pub fn open(
        descriptor: &PluginDescriptor,
        sample_rate: u32,
        monitor: Option<Arc<AudioMonitor>>,
    ) -> Result<Self, PluginError> {
        let plugin = crate::adapters::load_plugin(descriptor)?;
        let inner =
            EditorWindow::open(plugin, &descriptor.name, f64::from(sample_rate.max(1)), 512)
                .map_err(|error| PluginError::Load(error.to_string()))?;
        if let Some(monitor) = &monitor {
            monitor.set_enabled(true);
        }
        Ok(Self {
            inner,
            format: descriptor.format,
            plugin_uid: descriptor.uid.clone(),
            plugin_version: descriptor.version.clone(),
            monitor,
            monitor_block: vec![0.0; 512 * 2],
        })
    }

    pub fn focus(&self) {
        self.inner.focus();
    }

    pub fn pump(&mut self) {
        self.inner.pump();
        if self.inner.is_open() {
            self.drain_monitor();
        }
    }

    fn drain_monitor(&mut self) {
        let Some(monitor) = self.monitor.clone() else {
            return;
        };
        while let Some(block) = monitor.pop() {
            self.process_monitor_block(&block);
        }
    }

    fn process_monitor_block(&mut self, block: &MonitorBlock) {
        let frames = block.frames.min(512);
        if frames == 0 {
            return;
        }
        for frame in 0..frames {
            self.monitor_block[frame * 2] = block.samples[frame * 2];
            self.monitor_block[frame * 2 + 1] = block.samples[frame * 2 + 1];
        }
        let _ = self
            .inner
            .process_interleaved(&self.monitor_block[..frames * 2]);
    }

    pub fn is_open(&self) -> bool {
        self.inner.is_open()
    }

    /// Current content size of the hosted editor window.
    ///
    /// Updated from the plugin editor's reported size on every [`Self::pump`].
    pub fn size(&self) -> (u32, u32) {
        self.inner.size()
    }

    /// Returns and clears a playback-toggle request recorded while a plugin
    /// window was key. The Space key that triggered it is swallowed by the
    /// event monitor and never reaches the plugin editor.
    pub fn take_playback_toggle(&mut self) -> bool {
        self.inner.take_playback_toggle()
    }

    pub fn set_parameter(&mut self, id: u32, value: f64) {
        self.inner.set_parameter(id, value);
    }

    pub fn save_state(&mut self) -> Result<PluginState, PluginError> {
        let blob = self
            .inner
            .save_state()
            .map_err(|error| PluginError::Load(error.to_string()))?;
        Ok(PluginState {
            format: self.format,
            plugin_uid: self.plugin_uid.clone(),
            plugin_version: self.plugin_version.clone(),
            blob,
        })
    }

    pub fn load_state(&mut self, state: &PluginState) -> Result<(), PluginError> {
        if state.format != self.format || state.plugin_uid != self.plugin_uid {
            return Err(PluginError::StateIncompatible(
                "plugin state does not match the open editor".into(),
            ));
        }
        self.inner
            .load_state(&state.blob)
            .map_err(|error| PluginError::Load(error.to_string()))
    }

    pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
        self.inner.poll_changes()
    }

    /// Capture the editor content view as PNG bytes.
    pub fn capture_png(&self) -> Result<Vec<u8>, PluginError> {
        self.inner
            .capture_png()
            .map_err(|error| PluginError::Load(error.to_string()))
    }
}

impl Drop for NativePluginWindow {
    fn drop(&mut self) {
        if let Some(monitor) = &self.monitor {
            monitor.set_enabled(false);
        }
    }
}
