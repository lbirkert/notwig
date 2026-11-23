use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

use notwig_core::dsp::MonitorRegistry;
use notwig_core::{DeviceId, ParamId};
use notwig_plugins::NativePluginWindow;

use super::protocol::EngineReply;

pub(super) enum EditorCommand {
    Open {
        device_id: DeviceId,
        descriptor: notwig_plugins::PluginDescriptor,
        sample_rate: u32,
        state: Option<notwig_plugins::PluginState>,
        params: Vec<(ParamId, f64)>,
    },
    Close {
        device_id: DeviceId,
    },
    Focus {
        device_id: DeviceId,
    },
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: f64,
    },
    Shutdown,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(clippy::zero_sized_map_values, clippy::drop_non_drop)
)]
pub(super) struct EditorHost {
    monitors: Arc<MonitorRegistry>,
    windows: BTreeMap<DeviceId, NativePluginWindow>,
    replies: mpsc::Sender<EngineReply>,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(clippy::zero_sized_map_values, clippy::drop_non_drop)
)]
impl EditorHost {
    pub(super) fn new(replies: mpsc::Sender<EngineReply>, monitors: Arc<MonitorRegistry>) -> Self {
        Self {
            monitors,
            windows: BTreeMap::new(),
            replies,
        }
    }

    pub(super) fn open(
        &mut self,
        device_id: DeviceId,
        descriptor: notwig_plugins::PluginDescriptor,
        sample_rate: u32,
        state: Option<notwig_plugins::PluginState>,
        params: Vec<(ParamId, f64)>,
    ) {
        if let Some(window) = self.windows.get(&device_id) {
            window.focus();
            let _ = self.replies.send(EngineReply::EditorOpened { device_id });
            return;
        }
        let monitor = self.monitors.monitor(device_id);
        let result = NativePluginWindow::open(&descriptor, sample_rate, Some(monitor));
        match result {
            Ok(mut window) => {
                if let Some(state) = state
                    && let Err(error) = window.load_state(&state)
                {
                    let _ = self.replies.send(EngineReply::EditorClosed {
                        device_id,
                        state: None,
                        error: Some(error.to_string()),
                    });
                    return;
                }
                for (param_id, value) in params {
                    window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
                }
                self.windows.insert(device_id, window);
                let _ = self.replies.send(EngineReply::EditorOpened { device_id });
            }
            Err(error) => {
                let _ = self.replies.send(EngineReply::EditorClosed {
                    device_id,
                    state: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    pub(super) fn close(&mut self, device_id: DeviceId) {
        let Some(mut window) = self.windows.remove(&device_id) else {
            return;
        };
        let state = window.save_state().ok();
        drop(window);
        let _ = self.replies.send(EngineReply::EditorClosed {
            device_id,
            state,
            error: None,
        });
    }

    pub(super) fn focus(&self, device_id: DeviceId) {
        if let Some(window) = self.windows.get(&device_id) {
            window.focus();
        }
    }

    pub(super) fn set_param(&mut self, device_id: DeviceId, param_id: ParamId, value: f64) {
        if let Some(window) = self.windows.get_mut(&device_id) {
            window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
        }
    }

    pub(super) fn pump(&mut self) {
        let mut closed = Vec::new();
        let mut changes = Vec::new();
        let mut playback_toggle = false;
        for (device_id, window) in &mut self.windows {
            window.pump();
            playback_toggle |= window.take_playback_toggle();
            if !window.is_open() {
                closed.push(*device_id);
                continue;
            }
            for (raw_id, value) in window.poll_changes() {
                changes.push((*device_id, raw_id, value));
            }
        }
        if playback_toggle {
            let _ = self.replies.send(EngineReply::EditorTransportToggle);
        }
        for device_id in closed {
            self.close(device_id);
        }
        for (device_id, raw_id, value) in changes {
            let param_id = ParamId::from_uuid(uuid::Uuid::from_u128(u128::from(raw_id)));
            let _ = self.replies.send(EngineReply::EditorParam {
                device_id,
                param_id,
                value,
            });
        }
    }

    pub(super) fn close_all(&mut self) {
        let device_ids: Vec<_> = self.windows.keys().copied().collect();
        for device_id in device_ids {
            self.close(device_id);
        }
    }
}
