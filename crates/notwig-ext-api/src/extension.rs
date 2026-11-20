use std::collections::BTreeMap;

use notwig_core::ParamId;
use notwig_dsp::DeviceSpec;

use crate::abi::{AbiStatus, DeviceStore, packed_pointer, with_audio_frames};
use crate::panel::PanelDraw;

/// A device implemented by a WASM extension.
///
/// The host calls [`spec`](Self::spec) and [`panel`](Self::panel) once during
/// registration. Each device instance is created with [`new`](Self::new), then
/// receives prepare, parameter, and process calls through the ABI.
pub trait DeviceExtension: Send + Sized + 'static {
    fn new() -> Self;
    fn spec() -> DeviceSpec;
    fn panel() -> PanelDraw;
    fn prepare(&mut self, _sample_rate: u32, _channels: u16) {}
    fn set_param(&mut self, _param_id: ParamId, _value: f64) -> bool {
        false
    }
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32 {
        0
    }
}

trait DeviceInstance: Send {
    fn prepare(&mut self, sample_rate: u32, channels: u16);
    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool;
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32;
}

impl<T: DeviceExtension> DeviceInstance for T {
    fn prepare(&mut self, sample_rate: u32, channels: u16) {
        DeviceExtension::prepare(self, sample_rate, channels);
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        DeviceExtension::set_param(self, param_id, value)
    }

    fn process(&mut self, samples: &mut [f32]) {
        DeviceExtension::process(self, samples);
    }

    fn latency(&self) -> u32 {
        DeviceExtension::latency(self)
    }
}

struct RegisteredDevice {
    spec_json: Vec<u8>,
    panel_json: Vec<u8>,
    create: fn() -> Box<dyn DeviceInstance>,
}

struct DeviceSlot {
    type_id: String,
    channels: usize,
    instance: Box<dyn DeviceInstance>,
}

fn create_instance<D: DeviceExtension>() -> Box<dyn DeviceInstance> {
    Box::new(D::new())
}

/// Extension runtime shared by the generated ABI exports.
pub struct Extension {
    devices: BTreeMap<String, RegisteredDevice>,
    instances: DeviceStore<DeviceSlot>,
}

impl Extension {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            devices: BTreeMap::new(),
            instances: DeviceStore::new(),
        }
    }

    pub fn register_device<D: DeviceExtension>(&mut self) {
        let spec = D::spec();
        let panel = D::panel();
        let type_id = spec.type_id.clone();
        let spec_json = serde_json::to_vec(&spec).unwrap_or_default();
        let panel_json = serde_json::to_vec(&panel).unwrap_or_default();
        self.devices.insert(
            type_id,
            RegisteredDevice {
                spec_json,
                panel_json,
                create: create_instance::<D>,
            },
        );
    }

    #[must_use]
    pub fn describe_pointer(&self, type_id: &str) -> i64 {
        self.devices
            .get(type_id)
            .map_or(0, |device| packed_pointer(&device.spec_json))
    }

    pub fn create_device(&self, type_id: &str) -> i32 {
        let Some(device) = self.devices.get(type_id) else {
            return -1;
        };
        let instance = (device.create)();
        self.instances.create(DeviceSlot {
            type_id: type_id.to_owned(),
            channels: 2,
            instance,
        })
    }

    pub fn destroy_device(&self, handle: i32) {
        let _ = self.instances.remove(handle);
    }

    pub fn prepare(&self, handle: i32, sample_rate: u32, channels: u16) -> Result<(), AbiStatus> {
        self.instances.with(handle, |slot| {
            slot.channels = usize::from(channels.max(1));
            slot.instance.prepare(sample_rate, channels);
        })
    }

    pub fn set_param(&self, handle: i32, param_id: ParamId, value: f64) -> Result<bool, AbiStatus> {
        self.instances
            .with(handle, |slot| slot.instance.set_param(param_id, value))
    }

    pub fn process(&self, handle: i32, audio_ptr: i32, frames: usize) -> Result<(), AbiStatus> {
        let processed = self.instances.with(handle, |slot| {
            let channels = slot.channels.max(1);
            with_audio_frames(audio_ptr, frames, channels, |samples| {
                slot.instance.process(samples);
            })
        })?;
        processed.ok_or(AbiStatus::InvalidArgument)
    }

    #[must_use]
    pub fn latency(&self, handle: i32) -> u32 {
        self.instances
            .with(handle, |slot| slot.instance.latency())
            .unwrap_or(0)
    }

    #[must_use]
    pub fn panel_pointer(&self, handle: i32) -> i64 {
        let Ok(type_id) = self.instances.with(handle, |slot| slot.type_id.clone()) else {
            return 0;
        };
        self.devices
            .get(&type_id)
            .map_or(0, |device| packed_pointer(&device.panel_json))
    }
}

impl Default for Extension {
    fn default() -> Self {
        Self::new()
    }
}
