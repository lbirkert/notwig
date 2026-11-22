use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Db, DeviceChainId, DeviceId, Pan, ParamId, ParamValue, ProjectDoc, SendId, SoloContext,
    TrackId, TrackKind, TrackOutput,
};

use super::devices::{DeviceSpec, DeviceUiSpec, EngineCaps, RenderConfig};
use super::registry::DeviceRegistry;

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
    #[error("device error: {reason}")]
    DeviceError { reason: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanSend {
    pub id: SendId,
    pub target: TrackId,
    pub amount_db: Db,
    pub pre_fader: bool,
    pub enabled: bool,
    /// Extra delay applied to the send source so it enters the destination
    /// track's input at the same alignment as its other inputs.
    pub latency_samples: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanDevice {
    pub id: DeviceId,
    pub type_id: String,
    pub kind: crate::DeviceKind,
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
    pub active: bool,
    pub solo_effective: bool,
    pub parent: Option<TrackId>,
    pub output: TrackOutput,
    pub sends: Vec<PlanSend>,
    /// Latency of this track's own (active, non-bypassed) device chain.
    pub latency_samples: u32,
    /// Alignment delay applied to clip material before this track's devices so
    /// it lines up with routed and send input. Incoming connections are aligned
    /// to this same label by the engine.
    pub input_latency: u32,
    /// Timeline label of this track's output buffer:
    /// `input_latency + latency_samples`.
    pub output_latency: u32,
    /// Compensated latency from this track's input node to the master output.
    /// Equal to `total_latency - input_latency`, so the maximum over all tracks
    /// is the plan's `total_latency`.
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
                    ui: DeviceUiSpec::Generic,
                },
                false,
            ),
        };
        let latency_samples = if instance.bypass {
            0
        } else {
            spec.latency_samples
        };
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

    let solo = SoloContext::new(doc);
    let mut tracks = BTreeMap::new();
    for track in doc.tracks.values() {
        if !doc.chains.contains_key(&track.main_chain) {
            return Err(GraphError::MissingChain {
                track: track.id,
                chain: track.main_chain,
            });
        }
        let latency_samples = if track.active {
            chain_latency(doc, &devices, track.main_chain)
        } else {
            0
        };
        tracks.insert(
            track.id,
            PlanTrack {
                id: track.id,
                kind: track.kind,
                main_chain: track.main_chain,
                volume_db: track.mixer.volume_db,
                pan: track.mixer.pan,
                mute: track.mixer.mute,
                active: track.active,
                solo_effective: solo.is_effective(doc, track.id),
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
                        latency_samples: 0,
                    })
                    .collect(),
                latency_samples,
                input_latency: 0,
                output_latency: 0,
                latency_to_master: 0,
            },
        );
    }

    let order = doc.track_order.depth_first();
    let mut process_order: Vec<TrackId> = order.iter().rev().copied().collect();
    process_order.retain(|id| *id != master);
    process_order.push(master);

    let total_latency = compute_latency_labels(master, &mut tracks)?;

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
            let Some(device) = devices.get(id) else {
                continue;
            };
            if device.bypass {
                continue;
            }
            total = total.saturating_add(device.latency_samples);
            for child in &device.child_chains {
                total = total.saturating_add(chain_latency(doc, devices, *child));
            }
        }
    }
    total
}

#[derive(Clone, Copy)]
enum LatencyEdgeKind {
    Output,
    Send { pre_fader: bool },
}

#[derive(Clone, Copy)]
struct LatencyEdge {
    target: TrackId,
    kind: LatencyEdgeKind,
}

fn track_output_target(track: &PlanTrack, master: TrackId) -> Option<TrackId> {
    match track.output {
        TrackOutput::Track(target) => Some(target),
        TrackOutput::Default => track.parent.or((track.id != master).then_some(master)),
        TrackOutput::None => None,
    }
}

/// Aligns every input to a track against the latest input arriving there.
///
/// A track's `input_latency` is the maximum output label of all main-routed and
/// enabled send inputs (clip material starts at zero). Each connection is then
/// delayed by the difference between the destination's input label and its own
/// label, so the engine never needs to compensate twice along a path. The master
/// output label is the plan's total latency, and every track's clip material
/// reaches the master at that same label because clip input is delayed by
/// `input_latency`.
fn compute_latency_labels(
    master: TrackId,
    tracks: &mut BTreeMap<TrackId, PlanTrack>,
) -> Result<u32, GraphError> {
    let mut outgoing: BTreeMap<TrackId, Vec<LatencyEdge>> = BTreeMap::new();
    let mut indegree: BTreeMap<TrackId, usize> = tracks.keys().map(|id| (*id, 0)).collect();
    for track in tracks.values() {
        if let Some(target) = track_output_target(track, master) {
            if target != track.id && tracks.contains_key(&target) {
                outgoing.entry(track.id).or_default().push(LatencyEdge {
                    target,
                    kind: LatencyEdgeKind::Output,
                });
            }
        }
        for send in &track.sends {
            if send.enabled && send.target != track.id && tracks.contains_key(&send.target) {
                outgoing.entry(track.id).or_default().push(LatencyEdge {
                    target: send.target,
                    kind: LatencyEdgeKind::Send {
                        pre_fader: send.pre_fader,
                    },
                });
            }
        }
    }
    for edges in outgoing.values() {
        for edge in edges {
            if let Some(degree) = indegree.get_mut(&edge.target) {
                *degree += 1;
            }
        }
    }

    let mut ready: BTreeSet<TrackId> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(tracks.len());
    while let Some(id) = ready.iter().next().copied() {
        ready.remove(&id);
        order.push(id);
        if let Some(edges) = outgoing.get(&id) {
            for edge in edges {
                if let Some(degree) = indegree.get_mut(&edge.target) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        ready.insert(edge.target);
                    }
                }
            }
        }
    }
    if order.len() != tracks.len() {
        return Err(GraphError::Cycle);
    }

    let mut inputs: BTreeMap<TrackId, u32> = tracks.keys().map(|id| (*id, 0)).collect();
    let mut outputs: BTreeMap<TrackId, u32> = BTreeMap::new();
    for id in &order {
        let Some(track) = tracks.get(id) else {
            continue;
        };
        let input = inputs.get(id).copied().unwrap_or(0);
        let output = input.saturating_add(track.latency_samples);
        outputs.insert(*id, output);
        if let Some(edges) = outgoing.get(id) {
            for edge in edges {
                let label = match edge.kind {
                    LatencyEdgeKind::Output => output,
                    LatencyEdgeKind::Send { pre_fader } => {
                        if pre_fader {
                            input
                        } else {
                            output
                        }
                    }
                };
                if let Some(slot) = inputs.get_mut(&edge.target) {
                    *slot = (*slot).max(label);
                }
            }
        }
    }

    let total = outputs.get(&master).copied().unwrap_or(0);
    for (id, track) in tracks.iter_mut() {
        let input = inputs.get(id).copied().unwrap_or(0);
        track.input_latency = input;
        track.output_latency = outputs.get(id).copied().unwrap_or(input);
    }
    for track in tracks.values_mut() {
        let input = track.input_latency;
        let output = track.output_latency;
        for send in &mut track.sends {
            let target_input = inputs.get(&send.target).copied().unwrap_or(0);
            let label = if send.pre_fader { input } else { output };
            send.latency_samples = target_input.saturating_sub(label);
        }
    }
    for track in tracks.values_mut() {
        track.latency_to_master = total.saturating_sub(track.input_latency);
    }
    Ok(total)
}
