use crate::CoreError;
use crate::domain::{DeviceChain, DeviceInstance, DeviceKind, ProjectDoc};
use crate::ids::{DeviceChainId, DeviceId, ParamId};
use crate::protocol::{DocChange, EngineMsg, RampSpec};
use crate::tracks::TrackKind;

use super::dispatch::{Changes, not_found};

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_device_command(
    doc: &mut ProjectDoc,
    id: DeviceId,
    type_id: &str,
    version: Option<crate::time::SchemaVersion>,
    kind: DeviceKind,
    target_chain: DeviceChainId,
    index: Option<usize>,
    child_chain_ids: &[DeviceChainId],
    name: Option<String>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    insert_device(
        doc,
        id,
        type_id,
        version,
        kind,
        target_chain,
        index,
        child_chain_ids,
        name,
        changes,
    )?;
    Ok(Vec::new())
}

pub(super) fn remove_device_command(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    remove_device(doc, device_id, changes)?;
    Ok(Vec::new())
}

pub(super) fn move_device_command(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    to_chain: DeviceChainId,
    index: usize,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    move_device(doc, device_id, to_chain, index, changes)?;
    Ok(Vec::new())
}

pub(super) fn set_device_bypass(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    bypass: bool,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let device = doc
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| not_found(&device_id))?;
    device.bypass = bypass;
    changes.device(device_id);
    Ok(Vec::new())
}

pub(super) fn set_device_state(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    state_ref: Option<&crate::domain::StateBlobRef>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let device = doc
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| not_found(&device_id))?;
    device.state_ref = state_ref.cloned();
    changes.device(device_id);
    Ok(Vec::new())
}

pub(super) fn set_param(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    param_id: ParamId,
    value: &crate::domain::ParamValue,
    ramp: Option<RampSpec>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let device = doc
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| not_found(&device_id))?;
    device.param_values.insert(param_id, value.clone());
    changes.device(device_id);
    Ok(vec![EngineMsg::SetParam {
        device_id,
        param_id,
        value: value.clone(),
        ramp,
    }])
}

#[allow(clippy::too_many_arguments)]
fn insert_device(
    doc: &mut ProjectDoc,
    id: DeviceId,
    type_id: &str,
    version: Option<crate::time::SchemaVersion>,
    kind: DeviceKind,
    target_chain: DeviceChainId,
    index: Option<usize>,
    child_chain_ids: &[DeviceChainId],
    name: Option<String>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.devices.contains_key(&id) {
        return Err(CoreError::DuplicateId { id: id.to_string() });
    }
    if type_id.trim().is_empty() {
        return Err(CoreError::ValidationFailed {
            reason: "device type id must not be empty".into(),
        });
    }
    let chain = doc
        .chains
        .get(&target_chain)
        .cloned()
        .ok_or_else(|| not_found(&target_chain))?;
    for child in child_chain_ids {
        if doc.chains.contains_key(child) {
            return Err(CoreError::DuplicateId {
                id: child.to_string(),
            });
        }
    }
    let owner = doc
        .tracks
        .values()
        .find(|track| track.main_chain == target_chain)
        .cloned();
    let display_name = name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    if let Some(track) = &owner {
        match kind {
            DeviceKind::Instrument => {
                if track.kind != TrackKind::Instrument {
                    return Err(CoreError::ValidationFailed {
                        reason: "instrument devices require an Instrument track".into(),
                    });
                }
                if chain.devices.iter().any(|device_id| {
                    doc.devices
                        .get(device_id)
                        .is_some_and(|d| d.kind == DeviceKind::Instrument)
                }) {
                    return Err(CoreError::ValidationFailed {
                        reason: "an Instrument track may host at most one instrument".into(),
                    });
                }
            }
            DeviceKind::NoteFx => {
                if track.kind != TrackKind::Instrument {
                    return Err(CoreError::ValidationFailed {
                        reason: "NoteFx devices require an Instrument track".into(),
                    });
                }
            }
            DeviceKind::AudioFx | DeviceKind::Container => {}
        }
    }

    for child in child_chain_ids {
        doc.chains.insert(*child, DeviceChain::new(*child));
        changes.affected.chains.insert(*child);
        changes.push(DocChange::ChainUpserted(*child));
    }
    let instance = DeviceInstance {
        id,
        type_id: type_id.to_owned(),
        version: version.unwrap_or(crate::time::SchemaVersion::V0),
        kind,
        name,
        param_values: indexmap::IndexMap::new(),
        state_ref: None,
        bypass: false,
        child_chains: child_chain_ids.to_vec(),
    };
    doc.devices.insert(id, instance);
    let chain = doc.chains.get_mut(&target_chain).expect("chain exists");
    let index = index
        .unwrap_or(chain.devices.len())
        .min(chain.devices.len());
    chain.devices.insert(index, id);
    changes.device(id);
    changes.push(DocChange::ChainUpserted(target_chain));
    if let Some(owner) = &owner
        && owner.kind == TrackKind::Instrument
        && owner.name_is_auto
        && kind == DeviceKind::Instrument
        && let Some(display_name) = display_name
    {
        if let Some(track) = doc.tracks.get_mut(&owner.id) {
            track.name = display_name;
        }
        changes.track(owner.id);
    }
    doc.validate()
}

fn remove_device(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let Some(device) = doc.devices.get(&device_id).cloned() else {
        return Ok(());
    };
    for child in device.child_chains {
        remove_chain_recursive(doc, child, changes)?;
    }
    for chain in doc.chains.values_mut() {
        chain.devices.retain(|id| *id != device_id);
    }
    doc.devices.remove(&device_id);
    changes.affected.devices.insert(device_id);
    changes.push(DocChange::DeviceRemoved(device_id));
    Ok(())
}

pub(super) fn remove_chain_recursive(
    doc: &mut ProjectDoc,
    chain_id: DeviceChainId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if let Some(chain) = doc.chains.get(&chain_id).cloned() {
        for device_id in chain.devices {
            remove_device(doc, device_id, changes)?;
        }
    }
    doc.chains.remove(&chain_id);
    changes.affected.chains.insert(chain_id);
    changes.push(DocChange::ChainRemoved(chain_id));
    Ok(())
}

fn move_device(
    doc: &mut ProjectDoc,
    device_id: DeviceId,
    to_chain: DeviceChainId,
    index: usize,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if !doc.devices.contains_key(&device_id) {
        return Err(not_found(&device_id));
    }
    if !doc.chains.contains_key(&to_chain) {
        return Err(not_found(&to_chain));
    }
    for chain in doc.chains.values_mut() {
        chain.devices.retain(|id| *id != device_id);
    }
    let chain = doc.chains.get_mut(&to_chain).expect("chain exists");
    let index = index.min(chain.devices.len());
    chain.devices.insert(index, device_id);
    changes.device(device_id);
    changes.push(DocChange::ChainUpserted(to_chain));
    doc.validate()
}
