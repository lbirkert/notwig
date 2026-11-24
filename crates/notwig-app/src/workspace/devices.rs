use std::collections::BTreeMap;
use std::sync::Arc;

use crate::plugins::{PluginCatalog, new_plugin_host, plugin_entries};
use notwig_core::dsp::{AudioMonitor, DeviceFactory, DeviceSpec};
use notwig_core::ids::{Device as DeviceMarker, DeviceChain as DeviceChainMarker};
use notwig_core::protocol::CommandPayload;
use notwig_core::{
    DeviceChainId, DeviceId, DeviceKind, Id, SchemaVersion, StateBlobRef, TrackKind,
};
use notwig_plugins::{PluginDescriptor, PluginState};
use uuid::Uuid;

use super::{PLUGIN_STATE_ENCODING, Workspace};

impl Workspace {
    pub(super) fn duplicate_chain_devices(
        &mut self,
        from_chain: DeviceChainId,
        to_chain: DeviceChainId,
    ) -> Result<(), String> {
        let devices = self
            .doc
            .chains
            .get(&from_chain)
            .map(|chain| chain.devices.clone())
            .unwrap_or_default();
        for (index, device_id) in devices.into_iter().enumerate() {
            let Some(device) = self.doc.devices.get(&device_id).cloned() else {
                continue;
            };
            let new_device = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
            let child_chains: Vec<DeviceChainId> = device
                .child_chains
                .iter()
                .map(|_| Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4()))
                .collect();
            self.apply(CommandPayload::InsertDevice {
                id: new_device,
                type_id: device.type_id.clone(),
                version: Some(device.version),
                kind: device.kind,
                target_chain: to_chain,
                index: Some(index),
                child_chain_ids: child_chains.clone(),
                name: device.name.clone(),
            })?;
            for (old_child, new_child) in device.child_chains.iter().zip(&child_chains) {
                self.duplicate_chain_devices(*old_child, *new_child)?;
            }
            if device.bypass {
                self.apply(CommandPayload::SetDeviceBypass {
                    device_id: new_device,
                    bypass: true,
                })?;
            }
            if !device.active {
                self.apply(CommandPayload::SetDeviceActive {
                    device_id: new_device,
                    active: false,
                })?;
            }
            self.apply(CommandPayload::SetDeviceState {
                device_id: new_device,
                state_ref: device.state_ref.clone(),
            })?;
            for (param_id, value) in device.param_values {
                self.apply(CommandPayload::SetParam {
                    device_id: new_device,
                    param_id,
                    value,
                    ramp: None,
                })?;
            }
        }
        Ok(())
    }

    pub fn insert_stock(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
        index: Option<usize>,
    ) -> Result<(), String> {
        self.insert_device(chain_id, type_id, SchemaVersion::V0, kind, index)
    }

    pub fn move_device(
        &mut self,
        device_id: DeviceId,
        to_chain: DeviceChainId,
        index: usize,
    ) -> Result<(), String> {
        self.apply(CommandPayload::MoveDevice {
            device_id,
            to_chain,
            index,
        })
    }

    pub fn set_devices_active(
        &mut self,
        device_ids: &[DeviceId],
        active: bool,
    ) -> Result<(), String> {
        let payloads: Vec<CommandPayload> = device_ids
            .iter()
            .filter(|id| {
                self.doc
                    .devices
                    .get(id)
                    .is_some_and(|device| device.active != active)
            })
            .map(|id| CommandPayload::SetDeviceActive {
                device_id: *id,
                active,
            })
            .collect();
        if payloads.is_empty() {
            return Ok(());
        }
        self.apply_batch(payloads)
    }

    pub fn insert_device(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        version: SchemaVersion,
        kind: DeviceKind,
        index: Option<usize>,
    ) -> Result<(), String> {
        let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
        let child_chains = if kind == DeviceKind::Container {
            vec![Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4())]
        } else {
            Vec::new()
        };
        let name = if self
            .doc
            .tracks
            .values()
            .any(|track| track.main_chain == chain_id && track.kind == TrackKind::Instrument)
        {
            let registry = self.device_registry();
            registry
                .resolve(type_id, version)
                .or_else(|| registry.resolve_latest(type_id))
                .map(|spec| spec.name.clone())
        } else {
            None
        };
        self.apply(CommandPayload::InsertDevice {
            id,
            type_id: type_id.into(),
            version: Some(version),
            kind,
            target_chain: chain_id,
            index,
            child_chain_ids: child_chains,
            name,
        })
    }

    /// Audio tap used by an open plugin editor to drive meters and previews.
    pub fn monitor(&self, device_id: DeviceId) -> Arc<AudioMonitor> {
        self.monitors.monitor(device_id)
    }

    pub fn plugin_descriptor(&self, device_id: DeviceId) -> Option<PluginDescriptor> {
        let type_id = &self.doc.devices.get(&device_id)?.type_id;
        self.plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == *type_id)
            .cloned()
    }

    pub fn poll_plugin_catalog(&mut self) -> bool {
        if !PluginCatalog::global().poll() {
            return false;
        }
        self.plugin_descriptors = PluginCatalog::global().descriptors();
        self.plugins = plugin_entries(&self.plugin_descriptors);
        self.refresh_plugin_specs();
        if self.doc.devices.values().any(|device| {
            device.type_id.starts_with("vst3.")
                || device.type_id.starts_with("vst2.")
                || device.type_id.starts_with("au.")
                || device.type_id.starts_with("au3.")
                || device.type_id.starts_with("clap.")
                || device.type_id.starts_with("lv2.")
        }) {
            self.request_engine_rebuild();
        }
        true
    }

    pub fn plugin_specs(&self) -> impl Iterator<Item = &notwig_core::dsp::DeviceSpec> {
        self.plugin_specs.values()
    }

    pub(super) fn device_registry(&self) -> notwig_core::dsp::DeviceRegistry {
        let mut registry = notwig_core::dsp::stock_registry();
        for spec in self.plugin_specs.values() {
            registry.register(spec.clone());
        }
        for spec in self.extension_specs.values() {
            registry.register(spec.clone());
        }
        registry
    }

    pub fn set_extension_devices(
        &mut self,
        specs: Vec<DeviceSpec>,
        factory: Option<Arc<dyn DeviceFactory>>,
    ) {
        let specs: BTreeMap<String, DeviceSpec> = specs
            .into_iter()
            .map(|spec| (spec.type_id.clone(), spec))
            .collect();
        self.extension_specs = specs;
        self.extension_factory = factory;
        self.request_engine_rebuild();
    }

    pub fn extension_specs(&self) -> impl Iterator<Item = &DeviceSpec> {
        self.extension_specs.values()
    }

    pub(super) fn refresh_plugin_specs(&mut self) {
        let type_ids: Vec<String> = self
            .doc
            .devices
            .values()
            .filter(|device| {
                device.type_id.starts_with("vst3.")
                    || device.type_id.starts_with("vst2.")
                    || device.type_id.starts_with("au.")
                    || device.type_id.starts_with("au3.")
                    || device.type_id.starts_with("clap.")
                    || device.type_id.starts_with("lv2.")
            })
            .map(|device| device.type_id.clone())
            .collect();
        for type_id in type_ids {
            self.ensure_plugin_spec(&type_id);
        }
    }

    fn ensure_plugin_spec(&mut self, type_id: &str) {
        if self.plugin_specs.contains_key(type_id) {
            return;
        }
        let Some(descriptor) = self
            .plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == type_id)
            .cloned()
        else {
            return;
        };
        let mut host = new_plugin_host();
        let Ok(metadata) = host.probe_metadata(&descriptor) else {
            return;
        };
        let params = metadata.params;
        let capabilities = metadata.capabilities;
        tracing::debug!(
            plugin = %descriptor.name,
            params = params.len(),
            latency = capabilities.latency_samples,
            "loaded plugin parameter metadata"
        );
        self.plugin_specs.insert(
            type_id.to_owned(),
            notwig_core::dsp::DeviceSpec {
                type_id: type_id.to_owned(),
                version: notwig_core::SchemaVersion::V0,
                kind: if descriptor.is_instrument {
                    DeviceKind::Instrument
                } else {
                    DeviceKind::AudioFx
                },
                ports: Vec::new(),
                params,
                latency_samples: capabilities.latency_samples,
                name: descriptor.name,
                vendor: descriptor.vendor,
                ui: notwig_core::dsp::DeviceUiSpec::Generic,
                container: None,
            },
        );
    }

    pub fn insert_plugin(
        &mut self,
        chain_id: DeviceChainId,
        type_id: &str,
        kind: DeviceKind,
        index: Option<usize>,
    ) -> Result<DeviceId, String> {
        let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
        self.ensure_plugin_spec(type_id);
        let name = self
            .plugin_descriptors
            .iter()
            .find(|descriptor| descriptor.type_id == type_id)
            .map(|descriptor| descriptor.name.clone());
        self.apply(CommandPayload::InsertDevice {
            id,
            type_id: type_id.into(),
            version: None,
            kind,
            target_chain: chain_id,
            index,
            child_chain_ids: Vec::new(),
            name,
        })?;
        Ok(id)
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unused_self)]
    pub(super) fn load_plugin_states(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn load_plugin_states(&mut self) {
        self.plugin_states.clear();
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        for device in self.doc.devices.values() {
            let Some(state_ref) = &device.state_ref else {
                continue;
            };
            if state_ref.encoding != PLUGIN_STATE_ENCODING {
                continue;
            }
            let path = bundle
                .join("state")
                .join(format!("{}.json", state_ref.hash));
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            if let Ok(state) = serde_json::from_slice::<PluginState>(&bytes) {
                self.plugin_states.insert(device.id, state);
            }
        }
    }

    #[must_use]
    pub fn plugin_state(&self, device_id: DeviceId) -> Option<&PluginState> {
        self.plugin_states.get(&device_id)
    }

    pub fn store_plugin_state(&mut self, device_id: DeviceId, state: PluginState) {
        self.plugin_states.insert(device_id, state);
        if let Some(device) = self.doc.devices.get_mut(&device_id) {
            device.state_ref = Some(StateBlobRef {
                hash: device_id.to_string(),
                encoding: PLUGIN_STATE_ENCODING.into(),
            });
        }
        self.dirty = true;
    }

    pub fn set_plugin_state(&mut self, device_id: DeviceId, state: PluginState) {
        self.store_plugin_state(device_id, state);
        self.request_engine_rebuild();
    }
}
