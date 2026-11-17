use std::collections::{BTreeMap, BTreeSet};

use notwig_core::{
    Db, DeviceChainId, DeviceId, Pan, ParamId, ParamValue, ProjectDoc, SendId, TrackId, TrackKind,
    TrackOutput,
};

use crate::devices::{DeviceSpec, EngineCaps, RenderConfig};
use crate::registry::DeviceRegistry;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum GraphError {
    #[error("document validation failed: {reason}")]
    InvalidDocument { reason: String },
    #[error("unknown device type: {type_id}")]
    UnknownDevice { type_id: String },
    #[error("track {track} references missing chain {chain}")]
    MissingChain {
        track: TrackId,
        chain: DeviceChainId,
    },
    #[error("missing track: {id}")]
    MissingTrack { id: TrackId },
    #[error("project has no Master track")]
    NoMaster,
    #[error("processing graph contains a cycle")]
    Cycle,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanSend {
    pub id: SendId,
    pub target: TrackId,
    pub amount_db: Db,
    pub pre_fader: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanDevice {
    pub id: DeviceId,
    pub type_id: String,
    pub kind: notwig_core::DeviceKind,
    pub spec: DeviceSpec,
    pub param_values: BTreeMap<ParamId, ParamValue>,
    pub bypass: bool,
    pub child_chains: Vec<DeviceChainId>,
    pub latency_samples: u32,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanTrack {
    pub id: TrackId,
    pub kind: TrackKind,
    pub main_chain: DeviceChainId,
    pub volume_db: Db,
    pub pan: Pan,
    pub mute: bool,
    pub solo_effective: bool,
    pub parent: Option<TrackId>,
    pub output: TrackOutput,
    pub sends: Vec<PlanSend>,
    pub latency_samples: u32,
    pub latency_to_master: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompiledPlan {
    pub config: RenderConfig,
    pub master: TrackId,
    pub order: Vec<TrackId>,
    pub tracks: BTreeMap<TrackId, PlanTrack>,
    pub devices: BTreeMap<DeviceId, PlanDevice>,
    pub chains: BTreeMap<DeviceChainId, Vec<DeviceId>>,
    pub total_latency: u32,
}

impl CompiledPlan {
    #[must_use]
    pub fn chain_devices(&self, chain: DeviceChainId) -> &[DeviceId] {
        self.chains.get(&chain).map_or(&[], Vec::as_slice)
    }
}

pub fn compile(
    doc: &ProjectDoc,
    registry: &DeviceRegistry,
    caps: EngineCaps,
) -> Result<CompiledPlan, GraphError> {
    doc.validate().map_err(|err| GraphError::InvalidDocument {
        reason: err.to_string(),
    })?;
    let master = doc.master_track().ok_or(GraphError::NoMaster)?;
    let config = RenderConfig {
        sample_rate: doc.meta.sample_rate.value(),
        max_block_size: caps.max_block_size.max(1),
        channels: caps.channels.max(2),
        tempo_bpm: doc.meta.tempo_bpm,
    };

    let mut chains = BTreeMap::new();
    for chain in doc.chains.values() {
        chains.insert(chain.id, chain.devices.clone());
    }

    let mut devices = BTreeMap::new();
    for instance in doc.devices.values() {
        let (spec, resolved) = match registry.resolve(&instance.type_id, instance.version) {
            Some(spec) => (spec.clone(), true),
            None => (
                DeviceSpec {
                    type_id: instance.type_id.clone(),
                    version: instance.version,
                    kind: instance.kind,
                    ports: Vec::new(),
                    params: Vec::new(),
                    latency_samples: 0,
                    name: instance
                        .name
                        .clone()
                        .unwrap_or_else(|| instance.type_id.clone()),
                    vendor: String::new(),
                },
                false,
            ),
        };
        let latency_samples = spec.latency_samples;
        devices.insert(
            instance.id,
            PlanDevice {
                id: instance.id,
                type_id: instance.type_id.clone(),
                kind: instance.kind,
                spec,
                param_values: instance
                    .param_values
                    .iter()
                    .map(|(k, v)| (*k, v.clone()))
                    .collect(),
                bypass: instance.bypass,
                child_chains: instance.child_chains.clone(),
                latency_samples,
                resolved,
            },
        );
    }

    let any_solo = doc.tracks.values().any(|track| track.mixer.solo);
    let mut tracks = BTreeMap::new();
    for track in doc.tracks.values() {
        if !doc.chains.contains_key(&track.main_chain) {
            return Err(GraphError::MissingChain {
                track: track.id,
                chain: track.main_chain,
            });
        }
        let latency_samples = chain_latency(doc, &devices, track.main_chain);
        tracks.insert(
            track.id,
            PlanTrack {
                id: track.id,
                kind: track.kind,
                main_chain: track.main_chain,
                volume_db: track.mixer.volume_db,
                pan: track.mixer.pan,
                mute: track.mixer.mute,
                solo_effective: solo_effective(doc, track.id, any_solo),
                parent: track.parent,
                output: track.io.audio_out,
                sends: track
                    .sends
                    .iter()
                    .map(|send| PlanSend {
                        id: send.id,
                        target: send.target,
                        amount_db: send.amount_db,
                        pre_fader: send.pre_fader,
                        enabled: send.enabled,
                    })
                    .collect(),
                latency_samples,
                latency_to_master: 0,
            },
        );
    }

    let order = doc.track_order.depth_first();
    let mut process_order: Vec<TrackId> = order.iter().rev().copied().collect();
    process_order.retain(|id| *id != master);
    process_order.push(master);

    let mut memo = BTreeMap::new();
    let mut total_latency = 0;
    for id in tracks.keys().copied().collect::<Vec<_>>() {
        let latency = path_latency(doc, &tracks, id, master, &mut memo, &mut BTreeSet::new())?;
        total_latency = total_latency.max(latency);
        if let Some(track) = tracks.get_mut(&id) {
            track.latency_to_master = latency;
        }
    }

    Ok(CompiledPlan {
        config,
        master,
        order: process_order,
        tracks,
        devices,
        chains,
        total_latency,
    })
}

fn chain_latency(
    doc: &ProjectDoc,
    devices: &BTreeMap<DeviceId, PlanDevice>,
    chain: DeviceChainId,
) -> u32 {
    let mut total: u32 = 0;
    if let Some(devices_in_chain) = doc.chains.get(&chain) {
        for id in &devices_in_chain.devices {
            if let Some(device) = devices.get(id) {
                total = total.saturating_add(device.latency_samples);
                for child in &device.child_chains {
                    total = total.saturating_add(chain_latency(doc, devices, *child));
                }
            }
        }
    }
    total
}

fn solo_effective(doc: &ProjectDoc, id: TrackId, any_solo: bool) -> bool {
    if !any_solo {
        return true;
    }
    let Some(track) = doc.tracks.get(&id) else {
        return false;
    };
    if track.kind == TrackKind::Master || track.mixer.solo {
        return true;
    }
    let mut current = track.parent;
    while let Some(track_id) = current {
        let Some(parent) = doc.tracks.get(&track_id) else {
            break;
        };
        if parent.mixer.solo {
            return true;
        }
        current = parent.parent;
    }
    doc.track_order
        .descendants(id)
        .into_iter()
        .any(|child| doc.tracks.get(&child).is_some_and(|t| t.mixer.solo))
}

fn path_latency(
    doc: &ProjectDoc,
    tracks: &BTreeMap<TrackId, PlanTrack>,
    id: TrackId,
    master: TrackId,
    memo: &mut BTreeMap<TrackId, u32>,
    visiting: &mut BTreeSet<TrackId>,
) -> Result<u32, GraphError> {
    if let Some(value) = memo.get(&id) {
        return Ok(*value);
    }
    if !visiting.insert(id) {
        return Err(GraphError::Cycle);
    }
    let track = tracks.get(&id).ok_or(GraphError::MissingTrack { id })?;
    let own = track.latency_samples;
    let target = match track.output {
        TrackOutput::Track(target) => Some(target),
        TrackOutput::Default => track.parent.or((id != master).then_some(master)),
        TrackOutput::None => None,
    };
    let latency = if let Some(target) = target {
        if target == id {
            return Err(GraphError::Cycle);
        }
        own.saturating_add(path_latency(doc, tracks, target, master, memo, visiting)?)
    } else {
        own
    };
    visiting.remove(&id);
    memo.insert(id, latency);
    Ok(latency)
}
