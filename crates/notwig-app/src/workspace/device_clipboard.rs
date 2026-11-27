use std::collections::{BTreeMap, BTreeSet};

use notwig_core::ids::{Device as DeviceMarker, DeviceChain as DeviceChainMarker};
use notwig_core::protocol::CommandPayload;
use notwig_core::{
    DeviceChainId, DeviceId, DeviceInstance, DeviceKind, Id, ParamId, ParamValue, SchemaVersion,
    StateBlobRef,
};
use notwig_plugins::PluginState;
use uuid::Uuid;

use super::{PLUGIN_STATE_ENCODING, Workspace};

#[derive(Debug, Clone, Default)]
pub(super) struct DeviceClipboard {
    roots: Vec<DeviceSnapshot>,
}

#[derive(Debug, Clone)]
struct DeviceSnapshot {
    type_id: String,
    version: SchemaVersion,
    kind: DeviceKind,
    name: Option<String>,
    param_values: Vec<(ParamId, ParamValue)>,
    state_ref: Option<StateBlobRef>,
    bypass: bool,
    active: bool,
    child_chains: Vec<Vec<DeviceSnapshot>>,
    plugin_state: Option<PluginState>,
}

impl Workspace {
    /// Copies selected devices and their container subtrees into the device
    /// clipboard. Nested devices whose ancestor is also selected are not copied
    /// twice.
    pub fn copy_devices(&mut self, device_ids: &[DeviceId]) -> usize {
        let roots = self.device_roots(device_ids);
        let snapshots: Vec<DeviceSnapshot> = roots
            .iter()
            .filter_map(|id| self.snapshot_device(*id))
            .collect();
        let count = snapshots.len();
        self.device_clipboard = Some(DeviceClipboard { roots: snapshots });
        self.set_clipboard(notwig_ui::Clipboard::Devices);
        self.status = Some(if count == 1 {
            "Copied device.".into()
        } else {
            format!("Copied {count} devices.")
        });
        self.error = None;
        count
    }

    /// Copies selected devices and removes them in a single undo entry.
    pub fn cut_devices(&mut self, device_ids: &[DeviceId]) -> usize {
        let roots = self.device_roots(device_ids);
        let snapshots: Vec<DeviceSnapshot> = roots
            .iter()
            .filter_map(|id| self.snapshot_device(*id))
            .collect();
        let count = snapshots.len();
        if count == 0 {
            self.error = Some("No devices selected.".into());
            return 0;
        }
        let payloads = roots
            .iter()
            .map(|id| CommandPayload::RemoveDevice { device_id: *id })
            .collect();
        if let Err(err) = self.apply_batch(payloads) {
            self.error = Some(err);
            return 0;
        }
        self.device_clipboard = Some(DeviceClipboard { roots: snapshots });
        self.set_clipboard(notwig_ui::Clipboard::Devices);
        self.status = Some(if count == 1 {
            "Cut device.".into()
        } else {
            format!("Cut {count} devices.")
        });
        self.error = None;
        count
    }

    /// Pastes the device clipboard into `to_chain` starting at `index` and
    /// returns the new root device ids in clipboard order.
    pub fn paste_devices(
        &mut self,
        to_chain: DeviceChainId,
        index: usize,
    ) -> Result<Vec<DeviceId>, String> {
        let Some(clipboard) = self.device_clipboard.clone() else {
            return Err("Device clipboard is empty.".into());
        };
        if clipboard.roots.is_empty() {
            return Err("Device clipboard is empty.".into());
        }
        let mut commands = Vec::new();
        let mut plugin_states = BTreeMap::new();
        let mut roots = Vec::with_capacity(clipboard.roots.len());
        for (offset, snapshot) in clipboard.roots.iter().enumerate() {
            let id = snapshot_commands(
                snapshot,
                to_chain,
                index + offset,
                &mut commands,
                &mut plugin_states,
            );
            roots.push(id);
        }
        let inserted_states: Vec<DeviceId> = plugin_states.keys().copied().collect();
        for (device_id, state) in &plugin_states {
            self.plugin_states.insert(*device_id, state.clone());
        }
        if let Err(err) = self.apply_batch(commands) {
            for device_id in inserted_states {
                self.plugin_states.remove(&device_id);
            }
            return Err(err);
        }
        self.status = Some(if roots.len() == 1 {
            "Pasted device.".into()
        } else {
            format!("Pasted {} devices.", roots.len())
        });
        self.error = None;
        Ok(roots)
    }

    fn device_roots(&self, device_ids: &[DeviceId]) -> Vec<DeviceId> {
        let selected: BTreeSet<DeviceId> = device_ids
            .iter()
            .copied()
            .filter(|id| self.doc.devices.contains_key(id))
            .collect();
        if selected.is_empty() {
            return Vec::new();
        }
        let owners: BTreeMap<DeviceChainId, DeviceId> = self
            .doc
            .devices
            .values()
            .flat_map(|device| device.child_chains.iter().map(|chain| (*chain, device.id)))
            .collect();
        let mut order = BTreeMap::new();
        let mut index = 0usize;
        for chain in self.doc.chains.values() {
            for device_id in &chain.devices {
                order.insert(*device_id, index);
                index += 1;
            }
        }
        let mut roots: Vec<DeviceId> = selected
            .iter()
            .copied()
            .filter(|id| {
                let mut current = *id;
                let mut seen = BTreeSet::new();
                while let Some(parent) = self.parent_device(current, &owners) {
                    if !seen.insert(parent) || selected.contains(&parent) {
                        return false;
                    }
                    current = parent;
                }
                true
            })
            .collect();
        roots.sort_by_key(|id| order.get(id).copied().unwrap_or(usize::MAX));
        roots
    }

    fn parent_device(
        &self,
        device_id: DeviceId,
        owners: &BTreeMap<DeviceChainId, DeviceId>,
    ) -> Option<DeviceId> {
        self.doc
            .chains
            .values()
            .find(|chain| chain.devices.contains(&device_id))
            .and_then(|chain| owners.get(&chain.id).copied())
    }

    fn snapshot_device(&self, device_id: DeviceId) -> Option<DeviceSnapshot> {
        let device = self.doc.devices.get(&device_id)?;
        Some(self.snapshot_device_instance(device))
    }

    fn snapshot_device_instance(&self, device: &DeviceInstance) -> DeviceSnapshot {
        DeviceSnapshot {
            type_id: device.type_id.clone(),
            version: device.version,
            kind: device.kind,
            name: device.name.clone(),
            param_values: device
                .param_values
                .iter()
                .map(|(id, value)| (*id, value.clone()))
                .collect(),
            state_ref: device.state_ref.clone(),
            bypass: device.bypass,
            active: device.active,
            child_chains: device
                .child_chains
                .iter()
                .map(|chain_id| {
                    self.doc
                        .chains
                        .get(chain_id)
                        .map(|chain| {
                            chain
                                .devices
                                .iter()
                                .filter_map(|id| self.doc.devices.get(id))
                                .map(|device| self.snapshot_device_instance(device))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect(),
            plugin_state: self.plugin_states.get(&device.id).cloned(),
        }
    }
}

fn snapshot_commands(
    snapshot: &DeviceSnapshot,
    to_chain: DeviceChainId,
    index: usize,
    commands: &mut Vec<CommandPayload>,
    plugin_states: &mut BTreeMap<DeviceId, PluginState>,
) -> DeviceId {
    let id = Id::<DeviceMarker>::from_uuid(Uuid::new_v4());
    let child_chain_ids: Vec<DeviceChainId> = snapshot
        .child_chains
        .iter()
        .map(|_| Id::<DeviceChainMarker>::from_uuid(Uuid::new_v4()))
        .collect();
    commands.push(CommandPayload::InsertDevice {
        id,
        type_id: snapshot.type_id.clone(),
        version: Some(snapshot.version),
        kind: snapshot.kind,
        target_chain: to_chain,
        index: Some(index),
        child_chain_ids: child_chain_ids.clone(),
        name: snapshot.name.clone(),
    });
    if snapshot.bypass {
        commands.push(CommandPayload::SetDeviceBypass {
            device_id: id,
            bypass: true,
        });
    }
    if !snapshot.active {
        commands.push(CommandPayload::SetDeviceActive {
            device_id: id,
            active: false,
        });
    }
    let state_ref = snapshot.plugin_state.as_ref().map(|_| StateBlobRef {
        hash: id.to_string(),
        encoding: PLUGIN_STATE_ENCODING.into(),
    });
    if let Some(state) = &snapshot.plugin_state {
        plugin_states.insert(id, state.clone());
    }
    if state_ref.is_some() || snapshot.state_ref.is_some() {
        commands.push(CommandPayload::SetDeviceState {
            device_id: id,
            state_ref: state_ref.or_else(|| snapshot.state_ref.clone()),
        });
    }
    for (param_id, value) in &snapshot.param_values {
        commands.push(CommandPayload::SetParam {
            device_id: id,
            param_id: *param_id,
            value: value.clone(),
            ramp: None,
        });
    }
    for (child_chain_id, child_snapshots) in child_chain_ids.iter().zip(&snapshot.child_chains) {
        for (child_index, child) in child_snapshots.iter().enumerate() {
            snapshot_commands(child, *child_chain_id, child_index, commands, plugin_states);
        }
    }
    id
}
