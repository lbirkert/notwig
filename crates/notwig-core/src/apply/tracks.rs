use std::collections::BTreeSet;

use crate::CoreError;
use crate::domain::{DeviceChain, ProjectDoc, SoloContext};
use crate::ids::{DeviceChainId, SendId, TrackId};
use crate::protocol::{DocChange, EngineMsg};
use crate::sequencer::Lane;
use crate::tracks::{TrackKind, TrackOutput};

use super::devices::remove_chain_recursive;
use super::dispatch::{Changes, not_found, schedule_msg};

#[allow(clippy::too_many_arguments)]
pub(super) fn create_track_command(
    doc: &mut ProjectDoc,
    id: TrackId,
    chain_id: DeviceChainId,
    kind: TrackKind,
    name: String,
    parent: Option<TrackId>,
    index: Option<usize>,
    color: Option<crate::time::Color>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    create_track(doc, id, chain_id, kind, name, parent, index, color)?;
    changes.track(id);
    changes.push(DocChange::ChainUpserted(chain_id));
    changes.push(DocChange::TrackOrder(doc.track_order.clone()));
    Ok(Vec::new())
}

pub(super) fn delete_track_command(
    doc: &mut ProjectDoc,
    id: TrackId,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    delete_track(doc, id, changes)?;
    Ok(vec![schedule_msg(doc)])
}

pub(super) fn rename_track(
    doc: &mut ProjectDoc,
    id: TrackId,
    name: &str,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if name.trim().is_empty() {
        return Err(CoreError::ValidationFailed {
            reason: "track name must not be empty".into(),
        });
    }
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    name.clone_into(&mut track.name);
    track.name_is_auto = false;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn move_track_command(
    doc: &mut ProjectDoc,
    id: TrackId,
    parent: Option<TrackId>,
    index: usize,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    move_track(doc, id, parent, index)?;
    changes.track(id);
    changes.push(DocChange::TrackOrder(doc.track_order.clone()));
    Ok(Vec::new())
}

pub(super) fn set_track_color(
    doc: &mut ProjectDoc,
    id: TrackId,
    color: Option<crate::time::Color>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    track.color = color;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_comment(
    doc: &mut ProjectDoc,
    id: TrackId,
    comment: &str,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    comment.clone_into(&mut track.comment);
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_active(
    doc: &mut ProjectDoc,
    id: TrackId,
    active: bool,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    track.active = active;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_record_arm(
    doc: &mut ProjectDoc,
    id: TrackId,
    armed: bool,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    if armed && track.kind != TrackKind::Audio {
        return Err(CoreError::ValidationFailed {
            reason: "only audio tracks can be armed for recording".into(),
        });
    }
    track.record_armed = armed;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_input(
    doc: &mut ProjectDoc,
    id: TrackId,
    input: Option<&crate::tracks::AudioInputRef>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    if track.kind != TrackKind::Audio {
        return Err(CoreError::ValidationFailed {
            reason: "only audio tracks can select an audio input".into(),
        });
    }
    track.io.audio_input = input.cloned();
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_monitor_mode(
    doc: &mut ProjectDoc,
    id: TrackId,
    mode: crate::tracks::MonitorMode,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    if track.kind != TrackKind::Audio {
        return Err(CoreError::ValidationFailed {
            reason: "only audio tracks can select a monitoring mode".into(),
        });
    }
    track.io.monitor = mode;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_track_mixer(
    doc: &mut ProjectDoc,
    id: TrackId,
    volume_db: Option<crate::time::Db>,
    pan: Option<crate::time::Pan>,
    mute: Option<bool>,
    solo: Option<bool>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    if let Some(value) = volume_db {
        track.mixer.volume_db = value;
    }
    if let Some(value) = pan {
        track.mixer.pan = value;
    }
    if let Some(value) = mute {
        track.mixer.mute = value;
    }
    if let Some(value) = solo {
        track.mixer.solo = value;
    }
    changes.track(id);
    let solo = SoloContext::new(doc);
    Ok(doc
        .tracks
        .values()
        .map(|track| EngineMsg::SetTrackState {
            track_id: track.id,
            mute: track.mixer.mute,
            solo_effective: solo.is_effective(doc, track.id),
            gain_db: track.mixer.volume_db,
            pan: track.mixer.pan,
        })
        .collect())
}

pub(super) fn set_track_route(
    doc: &mut ProjectDoc,
    id: TrackId,
    audio_out: TrackOutput,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc.tracks.get_mut(&id).ok_or_else(|| not_found(&id))?;
    track.io.audio_out = audio_out;
    validate_routing(doc)?;
    changes.track(id);
    Ok(Vec::new())
}

pub(super) fn set_send(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    send: &crate::tracks::Send,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let target = doc
        .tracks
        .get(&send.target)
        .ok_or_else(|| not_found(&send.target))?;
    if !matches!(
        target.kind,
        TrackKind::BusFx | TrackKind::Group | TrackKind::Master
    ) {
        return Err(CoreError::ValidationFailed {
            reason: "send target must be BusFx, Group, or Master".into(),
        });
    }
    let track = doc
        .tracks
        .get_mut(&track_id)
        .ok_or_else(|| not_found(&track_id))?;
    if let Some(existing) = track.sends.iter_mut().find(|s| s.id == send.id) {
        *existing = send.clone();
    } else {
        track.sends.push(send.clone());
    }
    changes.track(track_id);
    Ok(Vec::new())
}

pub(super) fn remove_send(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    send_id: SendId,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let track = doc
        .tracks
        .get_mut(&track_id)
        .ok_or_else(|| not_found(&track_id))?;
    track.sends.retain(|send| send.id != send_id);
    changes.track(track_id);
    Ok(Vec::new())
}

pub fn create_track(
    doc: &mut ProjectDoc,
    id: TrackId,
    chain_id: DeviceChainId,
    kind: TrackKind,
    name: String,
    parent: Option<TrackId>,
    index: Option<usize>,
    color: Option<crate::time::Color>,
) -> Result<(), CoreError> {
    if doc.tracks.contains_key(&id) {
        return Err(CoreError::DuplicateId { id: id.to_string() });
    }
    if doc.chains.contains_key(&chain_id) {
        return Err(CoreError::DuplicateId {
            id: chain_id.to_string(),
        });
    }
    if name.trim().is_empty() {
        return Err(CoreError::ValidationFailed {
            reason: "track name must not be empty".into(),
        });
    }
    if kind == TrackKind::Master {
        if doc.master_track().is_some() {
            return Err(CoreError::Conflict {
                reason: "project already has a Master track".into(),
            });
        }
        if parent.is_some() {
            return Err(CoreError::ValidationFailed {
                reason: "Master must not have a parent".into(),
            });
        }
    } else if let Some(parent_id) = parent {
        let parent_track = doc
            .tracks
            .get(&parent_id)
            .ok_or_else(|| not_found(&parent_id))?;
        if !parent_track.kind.can_have_children() {
            return Err(CoreError::ValidationFailed {
                reason: "only Group tracks may be parents".into(),
            });
        }
    }

    doc.chains.insert(chain_id, DeviceChain::new(chain_id));
    let track = crate::tracks::Track {
        id,
        kind,
        name,
        name_is_auto: true,
        color,
        comment: String::new(),
        active: true,
        parent,
        mixer: crate::tracks::MixerState::default(),
        record_armed: false,
        io: crate::tracks::TrackIO {
            audio_out: if kind == TrackKind::Master {
                TrackOutput::None
            } else {
                TrackOutput::Default
            },
            ..crate::tracks::TrackIO::default()
        },
        main_chain: chain_id,
        sends: Vec::new(),
    };
    doc.tracks.insert(id, track);
    doc.track_order.insert(id, parent, index);
    if kind.has_lane() {
        doc.arranger.lanes.insert(id, Lane::default());
    }
    doc.validate()
}

fn delete_track(doc: &mut ProjectDoc, id: TrackId, changes: &mut Changes) -> Result<(), CoreError> {
    let track = doc.tracks.get(&id).ok_or_else(|| not_found(&id))?;
    if track.kind == TrackKind::Master {
        return Err(CoreError::ValidationFailed {
            reason: "Master track cannot be deleted".into(),
        });
    }
    let mut doomed = doc.track_order.descendants(id);
    doomed.push(id);
    let doomed_set: BTreeSet<TrackId> = doomed.iter().copied().collect();

    for track_id in &doomed {
        if let Some(lane) = doc.arranger.lanes.remove(track_id) {
            for clip_id in lane.clips {
                doc.arranger.clips.remove(&clip_id);
                changes.affected.clips.insert(clip_id);
                changes.push(DocChange::ClipRemoved(clip_id));
            }
        }
        if let Some(track) = doc.tracks.get(track_id) {
            let chain = track.main_chain;
            remove_chain_recursive(doc, chain, changes)?;
        }
    }
    for track_id in &doomed {
        doc.tracks.remove(track_id);
        doc.track_order.remove(*track_id);
        changes.affected.tracks.insert(*track_id);
        changes.push(DocChange::TrackRemoved(*track_id));
    }
    for track in doc.tracks.values_mut() {
        track
            .sends
            .retain(|send| !doomed_set.contains(&send.target));
    }
    doc.routing
        .connections
        .retain(|c| !doomed_set.contains(&c.from) && !doomed_set.contains(&c.to));
    changes.push(DocChange::TrackOrder(doc.track_order.clone()));
    changes.push(DocChange::Routing(doc.routing.clone()));
    doc.validate()
}

fn move_track(
    doc: &mut ProjectDoc,
    id: TrackId,
    parent: Option<TrackId>,
    index: usize,
) -> Result<(), CoreError> {
    let track = doc.tracks.get(&id).ok_or_else(|| not_found(&id))?;
    if track.kind == TrackKind::Master {
        return Err(CoreError::ValidationFailed {
            reason: "Master cannot be moved".into(),
        });
    }
    if let Some(parent_id) = parent {
        if parent_id == id {
            return Err(CoreError::ValidationFailed {
                reason: "a track cannot be its own parent".into(),
            });
        }
        let parent_track = doc
            .tracks
            .get(&parent_id)
            .ok_or_else(|| not_found(&parent_id))?;
        if !parent_track.kind.can_have_children() {
            return Err(CoreError::ValidationFailed {
                reason: "only Group tracks may be parents".into(),
            });
        }
        if doc.track_order.descendants(id).contains(&parent_id) {
            return Err(CoreError::ValidationFailed {
                reason: "moving track would create a hierarchy cycle".into(),
            });
        }
    }
    doc.track_order.move_node(id, parent, index)?;
    doc.tracks.get_mut(&id).expect("track exists").parent = parent;
    doc.validate()
}

fn validate_routing(doc: &ProjectDoc) -> Result<(), CoreError> {
    for track in doc.tracks.values() {
        let mut current = track.id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current) {
                return Err(CoreError::ValidationFailed {
                    reason: "audio routing graph contains a cycle".into(),
                });
            }
            let Some(node) = doc.tracks.get(&current) else {
                break;
            };
            match node.io.audio_out {
                TrackOutput::Track(next) => current = next,
                TrackOutput::Default => match node.parent {
                    Some(parent) => current = parent,
                    None => break,
                },
                TrackOutput::None => break,
            }
        }
    }
    Ok(())
}
