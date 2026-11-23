use std::collections::{BTreeMap, BTreeSet};

use notwig_core::dsp::{CompiledPlan, DeviceFactory, DeviceProcessor, PlanTrack};
use notwig_core::{DeviceId, TrackId, TrackOutput};

use crate::engine::EngineError;

pub(super) fn output_target(track: &PlanTrack, master: TrackId) -> Option<TrackId> {
    match track.output {
        TrackOutput::Track(target) => Some(target),
        TrackOutput::Default => track.parent.or((track.id != master).then_some(master)),
        TrackOutput::None => None,
    }
}

pub(super) fn processing_order(plan: &CompiledPlan) -> Result<Vec<TrackId>, EngineError> {
    let mut adjacency: BTreeMap<TrackId, BTreeSet<TrackId>> = BTreeMap::new();
    let mut indegree: BTreeMap<TrackId, usize> = plan.tracks.keys().map(|id| (*id, 0)).collect();
    let mut add_edge = |from: TrackId, to: TrackId| {
        if from == to || !plan.tracks.contains_key(&to) || !plan.tracks.contains_key(&from) {
            return;
        }
        let set = adjacency.entry(from).or_default();
        if set.insert(to) {
            *indegree.entry(to).or_insert(0) += 1;
        }
    };
    for track in plan.tracks.values() {
        let target = match track.output {
            TrackOutput::Track(target) => Some(target),
            TrackOutput::Default => track
                .parent
                .or((track.id != plan.master).then_some(plan.master)),
            TrackOutput::None => None,
        };
        if let Some(target) = target {
            add_edge(track.id, target);
        }
        for send in &track.sends {
            if send.enabled {
                add_edge(track.id, send.target);
            }
        }
    }
    let rank: BTreeMap<TrackId, usize> = plan
        .order
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut ready: BTreeSet<(usize, TrackId)> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| (rank.get(id).copied().unwrap_or(usize::MAX), *id))
        .collect();
    let mut order = Vec::with_capacity(plan.tracks.len());
    while let Some((_, id)) = ready.iter().next().copied() {
        ready.remove(&(rank.get(&id).copied().unwrap_or(usize::MAX), id));
        order.push(id);
        if let Some(targets) = adjacency.get(&id) {
            for target in targets.clone() {
                if let Some(degree) = indegree.get_mut(&target) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        ready.insert((rank.get(&target).copied().unwrap_or(usize::MAX), target));
                    }
                }
            }
        }
    }
    if order.len() != plan.tracks.len() {
        return Err(EngineError::Cycle);
    }
    Ok(order)
}

pub(super) fn build_processor(
    plan: &CompiledPlan,
    device_id: DeviceId,
    factory: &dyn DeviceFactory,
) -> Option<Box<dyn DeviceProcessor>> {
    let device = plan.devices.get(&device_id)?;
    if device.bypass {
        return None;
    }
    if device.type_id == notwig_core::dsp::stock::CHAIN_TYPE_ID {
        let mut chain = notwig_core::dsp::stock::ChainProcessor::new(&device.spec);
        for param in &device.spec.params {
            chain.set_param(param.id, &param.default);
        }
        for (id, value) in &device.param_values {
            chain.set_param(*id, value);
        }
        for child_chain in &device.child_chains {
            if let Some(children) = plan.chains.get(child_chain) {
                for child_id in children {
                    if let Some(child) = build_processor(plan, *child_id, factory) {
                        chain.add_child(*child_id, child);
                    }
                }
            }
        }
        return Some(Box::new(chain));
    }
    if device.kind == notwig_core::DeviceKind::Container {
        let mut container = notwig_core::dsp::ContainerProcessor::new(&device.spec);
        if let Some(stage) = factory.create(plan, device) {
            container.set_stage(stage);
        }
        for (id, value) in &device.param_values {
            container.set_param(*id, value);
        }
        for child_chain in &device.child_chains {
            if let Some(children) = plan.chains.get(child_chain) {
                for child_id in children {
                    if let Some(child) = build_processor(plan, *child_id, factory) {
                        container.add_child(*child_id, child);
                    }
                }
            }
        }
        return Some(Box::new(container));
    }
    let mut processor = factory.create(plan, device)?;
    for (id, value) in &device.param_values {
        processor.set_param(*id, value);
    }
    Some(processor)
}
