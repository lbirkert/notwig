use std::collections::BTreeMap;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use notwig_core::dsp::{AudioMonitor, MonitorBlock};
use notwig_rack::core::buffer::BusRange;
use notwig_rack::core::{
    AudioBuffer, BusLayout, EventList, Plugin, ProcessContext as RackContext, WindowHandle,
};
use objc2::msg_send;
use objc2::runtime::AnyObject;

use super::appkit::{create_window, lock_window_size, release_window};
use super::space_monitor::{
    PLAYBACK_TOGGLE_PENDING, register_plugin_window, unregister_plugin_window,
};
use crate::types::{PluginDescriptor, PluginError, PluginFormat, PluginState};

pub struct NativePluginWindow {
    plugin: Box<dyn Plugin<f32>>,
    format: PluginFormat,
    plugin_uid: String,
    plugin_version: String,
    window: *mut AnyObject,
    open: bool,
    params: BTreeMap<u32, usize>,
    last_values: BTreeMap<u32, f64>,
    last_poll: Instant,
    window_size: (u32, u32),
    monitor: Option<Arc<AudioMonitor>>,
    sample_rate: f64,
    monitor_input: [Vec<f32>; 2],
    monitor_output: [Vec<f32>; 2],
}

impl NativePluginWindow {
    pub fn open(
        descriptor: &PluginDescriptor,
        sample_rate: u32,
        monitor: Option<Arc<AudioMonitor>>,
    ) -> Result<Self, PluginError> {
        let mut plugin = crate::adapters::load_plugin(descriptor)?;
        let sample_rate = f64::from(sample_rate.max(1));
        let _ = plugin.activate(BusLayout::stereo(), sample_rate, 512);
        let mut params = BTreeMap::new();
        let mut last_values = BTreeMap::new();
        for index in 0..plugin.parameter_count() {
            if let Ok(info) = plugin.parameter_info(index) {
                params.insert(info.id, index);
                if let Ok(value) = plugin.parameter_value(index) {
                    last_values.insert(info.id, value);
                }
            }
        }
        let window = unsafe { create_window(&descriptor.name, 760.0, 560.0) }?;
        let content: *mut AnyObject = unsafe { msg_send![window, contentView] };
        if content.is_null() {
            unsafe {
                release_window(window);
            }
            return Err(PluginError::Load(
                "plugin window has no content view".into(),
            ));
        }
        let Some(editor) = plugin.editor() else {
            unsafe {
                release_window(window);
            }
            return Err(PluginError::Load("plugin has no editor".into()));
        };
        if let Err(err) = editor.open(WindowHandle::NSView(content.cast()), 1.0) {
            editor.close();
            unsafe {
                release_window(window);
            }
            return Err(PluginError::Load(err.to_string()));
        }
        let (plugin_size, resizable) = (
            editor.size().filter(|(w, h)| *w > 0 && *h > 0),
            editor.is_resizable(),
        );
        editor.show();
        let window_size = plugin_size.unwrap_or((760, 560));
        unsafe {
            lock_window_size(window, window_size, resizable);
        }
        unsafe {
            let _: () = msg_send![window, center];
            let _: () = msg_send![window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
        }
        register_plugin_window(window);
        if let Some(monitor) = &monitor {
            monitor.set_enabled(true);
        }
        Ok(Self {
            plugin,
            format: descriptor.format,
            plugin_uid: descriptor.uid.clone(),
            plugin_version: descriptor.version.clone(),
            window,
            open: true,
            params,
            last_values,
            last_poll: Instant::now(),
            window_size,
            monitor,
            sample_rate,
            monitor_input: std::array::from_fn(|_| vec![0.0; 512]),
            monitor_output: std::array::from_fn(|_| vec![0.0; 512]),
        })
    }

    pub fn focus(&self) {
        if self.open {
            unsafe {
                let _: () =
                    msg_send![self.window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
            }
        }
    }

    pub fn pump(&mut self) {
        if !self.open {
            return;
        }
        let visible: bool = unsafe { msg_send![self.window, isVisible] };
        if !visible {
            self.close();
            return;
        }
        let (plugin_size, resizable) = if let Some(editor) = self.plugin.editor() {
            editor.on_idle();
            (
                editor.size().filter(|(w, h)| *w > 0 && *h > 0),
                editor.is_resizable(),
            )
        } else {
            (None, false)
        };
        if let Some(size) = plugin_size
            && size != self.window_size
        {
            unsafe {
                lock_window_size(self.window, size, resizable);
            }
            self.window_size = size;
        }
        self.drain_monitor();
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
        for channel in &mut self.monitor_input {
            if channel.len() < frames {
                channel.resize(frames, 0.0);
            }
        }
        for channel in &mut self.monitor_output {
            if channel.len() < frames {
                channel.resize(frames, 0.0);
            }
            channel[..frames].fill(0.0);
        }
        for frame in 0..frames {
            self.monitor_input[0][frame] = block.samples[frame * 2];
            self.monitor_input[1][frame] = block.samples[frame * 2 + 1];
        }

        let input_refs: [&[f32]; 2] = [
            &self.monitor_input[0][..frames],
            &self.monitor_input[1][..frames],
        ];
        let (left, right) = self.monitor_output.split_at_mut(1);
        let mut output_refs: [&mut [f32]; 2] = [&mut left[0][..frames], &mut right[0][..frames]];
        let buses = [BusRange::new(0, 2)];
        let mut buffer = AudioBuffer::new(&input_refs, &mut output_refs, frames, &buses, &buses);
        let events = EventList::new();
        let mut output_events = EventList::new();
        let mut context = RackContext {
            sample_rate: self.sample_rate,
            max_block_size: 512,
            transport: None,
            output_events: &mut output_events,
        };
        let _ = self.plugin.process(&mut buffer, &events, &mut context);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Current content size of the hosted editor window.
    ///
    /// Updated from the plugin editor's reported size on every [`Self::pump`].
    pub fn size(&self) -> (u32, u32) {
        self.window_size
    }

    /// Returns and clears a playback-toggle request recorded while a plugin
    /// window was key. The Space key that triggered it is swallowed by the
    /// event monitor and never reaches the plugin editor.
    pub fn take_playback_toggle(&mut self) -> bool {
        PLAYBACK_TOGGLE_PENDING.swap(false, Ordering::AcqRel)
    }

    pub fn set_parameter(&mut self, id: u32, value: f64) {
        if let Some(index) = self.params.get(&id).copied() {
            let _ = self.plugin.set_parameter(index, value);
            self.last_values.insert(id, value);
        }
    }

    pub fn save_state(&mut self) -> Result<PluginState, PluginError> {
        let blob = self
            .plugin
            .save_state()
            .map_err(|err| PluginError::Load(err.to_string()))?;
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
        self.plugin
            .load_state(&state.blob)
            .map_err(|err| PluginError::Load(err.to_string()))
    }

    pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
        if self.last_poll.elapsed() < Duration::from_millis(50) {
            return Vec::new();
        }
        self.last_poll = Instant::now();
        let mut changes = Vec::new();
        for (id, index) in &self.params {
            let Ok(value) = self.plugin.parameter_value(*index) else {
                continue;
            };
            let differs = self
                .last_values
                .get(id)
                .is_none_or(|previous| (previous - value).abs() > 1.0e-4);
            if differs {
                changes.push((*id, value));
                self.last_values.insert(*id, value);
            }
        }
        changes
    }

    fn close(&mut self) {
        if !self.open {
            return;
        }
        if let Some(editor) = self.plugin.editor() {
            editor.close();
        }
        if let Some(monitor) = &self.monitor {
            monitor.set_enabled(false);
        }
        unregister_plugin_window(self.window);
        unsafe {
            let _: () = msg_send![self.window, orderOut: ptr::null_mut::<AnyObject>()];
            release_window(self.window);
        }
        self.window = ptr::null_mut();
        self.open = false;
    }
}

impl Drop for NativePluginWindow {
    fn drop(&mut self) {
        self.close();
    }
}
