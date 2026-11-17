use std::collections::BTreeSet;
use std::sync::Arc;

use crate::CoreError;
use crate::domain::{DeviceChain, DeviceInstance, DeviceKind, ProjectDoc};
use crate::ids::{ClipId, DeviceChainId, DeviceId, NoteId, TrackId};
use crate::protocol::{
    AffectedSet, AudioEdit, Command, CommandPayload, DocChange, EngineMsg, PlaybackSchedule,
    StateDelta,
};
use crate::sequencer::{
    ClipEntity, ClipKind, ClipPayload, Lane, MidiClipData, NoteEntity, sort_lane,
};
use crate::time::{RangeMus, TimeMus};
use crate::tracks::{TrackKind, TrackOutput};

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyResult {
    pub doc: ProjectDoc,
    pub delta: StateDelta,
    pub engine_msgs: Vec<EngineMsg>,
}

#[derive(Default)]
struct Changes {
    changes: Vec<DocChange>,
    affected: AffectedSet,
}

impl Changes {
    fn track(&mut self, _doc: &ProjectDoc, id: TrackId) {
        self.affected.tracks.insert(id);
        self.changes.push(DocChange::TrackUpserted(id));
    }

    fn device(&mut self, id: DeviceId) {
        self.affected.devices.insert(id);
        self.changes.push(DocChange::DeviceUpserted(id));
    }

    fn push(&mut self, change: DocChange) {
        self.changes.push(change);
    }
}

pub fn apply(doc: &ProjectDoc, command: &Command) -> Result<ApplyResult, CoreError> {
    let mut next = doc.clone();
    let mut changes = Changes::default();
    let mut engine_msgs = apply_payload(&mut next, &command.payload, &mut changes)?;
    let rev = next.allocate_revision();
    next.meta.modified_utc = next.meta.modified_utc.max(next.meta.created_utc);
    engine_msgs.push(EngineMsg::UpdateSchedule {
        schedule: Arc::new(PlaybackSchedule::from_doc(&next)),
    });
    Ok(ApplyResult {
        doc: next,
        delta: StateDelta {
            rev,
            changes: changes.changes,
            affected: changes.affected,
        },
        engine_msgs,
    })
}

fn apply_payload(
    doc: &mut ProjectDoc,
    payload: &CommandPayload,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    match payload {
        CommandPayload::SetTransport {
            playing,
            playhead,
            loop_enabled,
            loop_range,
        } => {
            if let Some(value) = playing {
                doc.transport.playing = *value;
            }
            if let Some(value) = playhead {
                if *value < TimeMus::zero() {
                    return Err(CoreError::ValidationFailed {
                        reason: "playhead must not be negative".into(),
                    });
                }
                doc.transport.playhead = *value;
            }
            if let Some(value) = loop_enabled {
                doc.transport.loop_enabled = *value;
            }
            if let Some(value) = loop_range {
                doc.transport.loop_range = *value;
            }
            doc.transport.validate()?;
            changes.push(DocChange::Transport(doc.transport.clone()));
            Ok(vec![transport_msg(doc)])
        }
        CommandPayload::Pause => {
            doc.transport.playing = false;
            changes.push(DocChange::Transport(doc.transport.clone()));
            Ok(vec![transport_msg(doc)])
        }
        CommandPayload::Stop => {
            doc.transport.playing = false;
            doc.transport.playhead = match doc.transport.loop_range {
                Some(range) if doc.transport.loop_enabled => range.start,
                _ => TimeMus::zero(),
            };
            changes.push(DocChange::Transport(doc.transport.clone()));
            Ok(vec![transport_msg(doc), EngineMsg::AllNotesOff])
        }
        CommandPayload::CreateTrack {
            id,
            chain_id,
            kind,
            name,
            parent,
            index,
            color,
        } => {
            create_track(
                doc,
                *id,
                *chain_id,
                *kind,
                name.clone(),
                *parent,
                *index,
                *color,
            )?;
            changes.track(doc, *id);
            changes.push(DocChange::ChainUpserted(*chain_id));
            changes.push(DocChange::TrackOrder(doc.track_order.clone()));
            Ok(Vec::new())
        }
        CommandPayload::DeleteTrack { id } => {
            delete_track(doc, *id, changes)?;
            Ok(Vec::new())
        }
        CommandPayload::RenameTrack { id, name } => {
            if name.trim().is_empty() {
                return Err(CoreError::ValidationFailed {
                    reason: "track name must not be empty".into(),
                });
            }
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.name = name.clone();
            changes.track(doc, *id);
            Ok(Vec::new())
        }
        CommandPayload::MoveTrack { id, parent, index } => {
            move_track(doc, *id, *parent, *index)?;
            changes.track(doc, *id);
            changes.push(DocChange::TrackOrder(doc.track_order.clone()));
            Ok(Vec::new())
        }
        CommandPayload::SetTrackMixer {
            id,
            volume_db,
            pan,
            mute,
            solo,
        } => {
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            if let Some(value) = volume_db {
                track.mixer.volume_db = *value;
            }
            if let Some(value) = pan {
                track.mixer.pan = *value;
            }
            if let Some(value) = mute {
                track.mixer.mute = *value;
            }
            if let Some(value) = solo {
                track.mixer.solo = *value;
            }
            changes.track(doc, *id);
            let solo_effective = effective_solo(doc, *id);
            Ok(vec![EngineMsg::SetTrackState {
                track_id: *id,
                mute: doc.tracks[id].mixer.mute,
                solo_effective,
                gain_db: doc.tracks[id].mixer.volume_db,
                pan: doc.tracks[id].mixer.pan,
            }])
        }
        CommandPayload::SetTrackRoute { id, audio_out } => {
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.io.audio_out = *audio_out;
            validate_routing(doc)?;
            changes.track(doc, *id);
            Ok(Vec::new())
        }
        CommandPayload::SetSend { track_id, send } => {
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
                .get_mut(track_id)
                .ok_or_else(|| not_found(track_id))?;
            if let Some(existing) = track.sends.iter_mut().find(|s| s.id == send.id) {
                *existing = send.clone();
            } else {
                if track.sends.iter().any(|s| s.id == send.id) {
                    return Err(CoreError::DuplicateId {
                        id: send.id.to_string(),
                    });
                }
                track.sends.push(send.clone());
            }
            changes.track(doc, *track_id);
            Ok(Vec::new())
        }
        CommandPayload::RemoveSend { track_id, send_id } => {
            let track = doc
                .tracks
                .get_mut(track_id)
                .ok_or_else(|| not_found(track_id))?;
            track.sends.retain(|send| send.id != *send_id);
            changes.track(doc, *track_id);
            Ok(Vec::new())
        }
        CommandPayload::CreateClip {
            id,
            track_id,
            kind,
            range,
            payload,
        } => {
            create_clip(doc, *id, *track_id, *kind, *range, payload.clone(), changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::EditClip {
            clip_id,
            range,
            name,
            muted,
            content_start,
            content_length,
            audio,
        } => {
            edit_clip(
                doc,
                *clip_id,
                *range,
                name.clone(),
                *muted,
                *content_start,
                *content_length,
                audio.as_ref(),
                changes,
            )?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::MoveClip {
            clip_id,
            track_id,
            range,
        } => {
            move_clip(doc, *clip_id, *track_id, *range, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SplitClip {
            clip_id,
            at,
            new_clip_id,
            new_note_ids,
        } => {
            split_clip(doc, *clip_id, *at, *new_clip_id, new_note_ids, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::DuplicateClip {
            clip_id,
            new_clip_id,
            note_id_map,
            track_id,
            range,
        } => {
            duplicate_clip(
                doc,
                *clip_id,
                *new_clip_id,
                note_id_map,
                *track_id,
                *range,
                changes,
            )?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::DeleteClip { clip_id } => {
            delete_clip(doc, *clip_id, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetNote { clip_id, note } => {
            set_note(doc, *clip_id, note.clone(), changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::DeleteNote { clip_id, note_id } => {
            delete_note(doc, *clip_id, *note_id, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::InsertDevice {
            id,
            type_id,
            version,
            kind,
            target_chain,
            index,
            child_chain_ids,
            name,
        } => {
            insert_device(
                doc,
                *id,
                type_id,
                *version,
                *kind,
                *target_chain,
                *index,
                child_chain_ids,
                name.clone(),
                changes,
            )?;
            Ok(Vec::new())
        }
        CommandPayload::RemoveDevice { device_id } => {
            remove_device(doc, *device_id, changes)?;
            Ok(Vec::new())
        }
        CommandPayload::MoveDevice {
            device_id,
            to_chain,
            index,
        } => {
            move_device(doc, *device_id, *to_chain, *index, changes)?;
            Ok(Vec::new())
        }
        CommandPayload::SetDeviceBypass { device_id, bypass } => {
            let device = doc
                .devices
                .get_mut(device_id)
                .ok_or_else(|| not_found(device_id))?;
            device.bypass = *bypass;
            changes.device(*device_id);
            Ok(Vec::new())
        }
        CommandPayload::SetDeviceState {
            device_id,
            state_ref,
        } => {
            let device = doc
                .devices
                .get_mut(device_id)
                .ok_or_else(|| not_found(device_id))?;
            device.state_ref = state_ref.clone();
            changes.device(*device_id);
            Ok(Vec::new())
        }
        CommandPayload::SetParam {
            device_id,
            param_id,
            value,
            ramp,
        } => {
            let device = doc
                .devices
                .get_mut(device_id)
                .ok_or_else(|| not_found(device_id))?;
            device.param_values.insert(*param_id, value.clone());
            changes.device(*device_id);
            Ok(vec![EngineMsg::SetParam {
                device_id: *device_id,
                param_id: *param_id,
                value: value.clone(),
                ramp: *ramp,
            }])
        }
        CommandPayload::RegisterAsset { asset_id, entry } => {
            if entry.id != *asset_id {
                return Err(CoreError::ValidationFailed {
                    reason: "asset id does not match entry id".into(),
                });
            }
            if doc.assets.assets.contains_key(asset_id) {
                return Err(CoreError::DuplicateId {
                    id: asset_id.to_string(),
                });
            }
            doc.assets.assets.insert(*asset_id, entry.clone());
            changes.affected.assets.insert(*asset_id);
            changes.push(DocChange::AssetUpserted(*asset_id));
            Ok(Vec::new())
        }
        CommandPayload::RelinkAsset { asset_id, uri } => {
            let entry = doc
                .assets
                .assets
                .get_mut(asset_id)
                .ok_or_else(|| not_found(asset_id))?;
            entry.uri = uri.clone();
            changes.affected.assets.insert(*asset_id);
            changes.push(DocChange::AssetUpserted(*asset_id));
            Ok(Vec::new())
        }
    }
}

fn not_found<T: std::fmt::Display>(id: &T) -> CoreError {
    CoreError::NotFound { id: id.to_string() }
}

fn transport_msg(doc: &ProjectDoc) -> EngineMsg {
    let map = doc.tempo_map();
    let playhead_samples = map.mus_to_samples(doc.transport.playhead);
    let (start, end) = doc.transport.loop_range.map_or((None, None), |range| {
        (
            Some(map.mus_to_samples(range.start)),
            Some(map.mus_to_samples(range.end)),
        )
    });
    EngineMsg::SetTransport {
        playing: doc.transport.playing,
        playhead_samples,
        loop_enabled: doc.transport.loop_enabled,
        loop_start_samples: start,
        loop_end_samples: end,
        stop: false,
    }
}

fn schedule_msg(doc: &ProjectDoc) -> EngineMsg {
    EngineMsg::UpdateSchedule {
        schedule: Arc::new(PlaybackSchedule::from_doc(doc)),
    }
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
        color,
        parent,
        mixer: crate::tracks::MixerState::default(),
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

fn remove_chain_recursive(
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

fn effective_solo(doc: &ProjectDoc, id: TrackId) -> bool {
    let any_solo = doc.tracks.values().any(|t| t.mixer.solo);
    if !any_solo {
        return true;
    }
    let mut current = Some(id);
    while let Some(track_id) = current {
        let Some(track) = doc.tracks.get(&track_id) else {
            break;
        };
        if track.mixer.solo {
            return true;
        }
        current = track.parent;
    }
    false
}

fn create_clip(
    doc: &mut ProjectDoc,
    id: ClipId,
    track_id: TrackId,
    kind: ClipKind,
    range: RangeMus,
    payload: ClipPayload,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&id) {
        return Err(CoreError::DuplicateId { id: id.to_string() });
    }
    let track = doc
        .tracks
        .get(&track_id)
        .ok_or_else(|| not_found(&track_id))?;
    if !track.kind.has_lane() {
        return Err(CoreError::ValidationFailed {
            reason: "only Audio and Instrument tracks host clips".into(),
        });
    }
    let clip = ClipEntity {
        id,
        track_id,
        kind,
        range,
        name: None,
        color: None,
        muted: false,
        payload,
    };
    clip.validate(doc)?;
    doc.arranger.clips.insert(id, clip);
    let lane = doc.arranger.lanes.entry(track_id).or_default();
    lane.clips.push(id);
    resolve_overlaps(doc, track_id, id, range, changes)?;
    changes.affected.clips.insert(id);
    changes.push(DocChange::ClipUpserted(id));
    doc.validate()
}

fn resolve_overlaps(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    authoritative: ClipId,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let Some(lane) = doc.arranger.lanes.get(&track_id).cloned() else {
        return Ok(());
    };
    let mut remove = Vec::new();
    for other in lane.clips {
        if other == authoritative {
            continue;
        }
        let Some(existing) = doc.arranger.clips.get(&other).cloned() else {
            continue;
        };
        if !existing.range.intersects(range) {
            continue;
        }
        if existing.range.start < range.start && existing.range.end > range.end {
            trim_clip_start(doc, &existing, range.end)?;
            changes.affected.clips.insert(other);
            changes.push(DocChange::ClipUpserted(other));
        } else if existing.range.start >= range.start && existing.range.end <= range.end {
            remove.push(other);
        } else if existing.range.start < range.start {
            trim_clip_end(doc, &existing, range.start)?;
            changes.affected.clips.insert(other);
            changes.push(DocChange::ClipUpserted(other));
        } else {
            trim_clip_start(doc, &existing, range.end)?;
            changes.affected.clips.insert(other);
            changes.push(DocChange::ClipUpserted(other));
        }
    }
    for id in remove {
        remove_clip_internal(doc, id, changes);
    }
    if let Some(lane) = doc.arranger.lanes.get_mut(&track_id) {
        sort_lane(lane, &doc.arranger.clips);
    }
    Ok(())
}

fn remove_clip_internal(doc: &mut ProjectDoc, id: ClipId, changes: &mut Changes) {
    if let Some(clip) = doc.arranger.clips.remove(&id) {
        if let Some(lane) = doc.arranger.lanes.get_mut(&clip.track_id) {
            lane.clips.retain(|clip_id| *clip_id != id);
        }
        changes.affected.clips.insert(id);
        changes.push(DocChange::ClipRemoved(id));
    }
}

fn trim_clip_start(
    doc: &mut ProjectDoc,
    clip: &ClipEntity,
    new_start: TimeMus,
) -> Result<(), CoreError> {
    let delta = new_start - clip.range.start;
    let mut next = clip.clone();
    next.range = RangeMus::new(new_start, clip.range.end)?;
    match &mut next.payload {
        ClipPayload::Midi(midi) => {
            midi.content_start = midi.content_start + delta;
        }
        ClipPayload::Audio(audio) => {
            let samples = doc.tempo_map().samples_delta(delta);
            audio.source_start_sample = audio
                .source_start_sample
                .saturating_add(u64::try_from(samples.max(0)).unwrap_or(0));
        }
    }
    if next.range.is_empty() {
        return Ok(());
    }
    next.validate(doc)?;
    doc.arranger.clips.insert(clip.id, next);
    Ok(())
}

fn trim_clip_end(
    doc: &mut ProjectDoc,
    clip: &ClipEntity,
    new_end: TimeMus,
) -> Result<(), CoreError> {
    let mut next = clip.clone();
    next.range = RangeMus::new(clip.range.start, new_end)?;
    match &mut next.payload {
        ClipPayload::Midi(_) => {}
        ClipPayload::Audio(audio) => {
            let delta = clip.range.end - new_end;
            let samples = doc.tempo_map().samples_delta(delta);
            audio.source_end_sample = audio
                .source_end_sample
                .saturating_sub(u64::try_from(samples.max(0)).unwrap_or(0));
        }
    }
    if next.range.is_empty() {
        return Ok(());
    }
    next.validate(doc)?;
    doc.arranger.clips.insert(clip.id, next);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn edit_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    range: Option<RangeMus>,
    name: Option<Option<String>>,
    muted: Option<bool>,
    content_start: Option<TimeMus>,
    content_length: Option<TimeMus>,
    audio: Option<&AudioEdit>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let mut next = existing.clone();
    if let Some(value) = name {
        next.name = value;
    }
    if let Some(value) = muted {
        next.muted = value;
    }
    if let Some(new_range) = range {
        if let ClipPayload::Audio(data) = &mut next.payload {
            let old = existing.range;
            let left_mus = new_range.start - old.start;
            let right_mus = old.end - new_range.end;
            let left_samples = doc.tempo_map().samples_delta(left_mus);
            let right_samples = doc.tempo_map().samples_delta(right_mus);
            let new_start = i128::from(data.source_start_sample) + i128::from(left_samples);
            let new_end = i128::from(data.source_end_sample) - i128::from(right_samples);
            if new_start < 0 || new_end <= new_start {
                return Err(CoreError::ValidationFailed {
                    reason: "audio trim exceeds source material".into(),
                });
            }
            data.source_start_sample = u64::try_from(new_start).unwrap_or(0);
            data.source_end_sample = u64::try_from(new_end).unwrap_or(0);
        }
        next.range = new_range;
    }
    match &mut next.payload {
        ClipPayload::Midi(midi) => {
            if let Some(value) = content_start {
                midi.content_start = value;
            }
            if let Some(value) = content_length {
                midi.length = value;
            }
        }
        ClipPayload::Audio(data) => {
            if let Some(edit) = audio {
                if let Some(value) = edit.source_start_sample {
                    data.source_start_sample = value;
                }
                if let Some(value) = edit.source_end_sample {
                    data.source_end_sample = value;
                }
                if let Some(value) = edit.gain_db {
                    data.gain_db = value;
                }
                if let Some(value) = edit.fade_in_samples {
                    data.fade_in_samples = value;
                }
                if let Some(value) = edit.fade_out_samples {
                    data.fade_out_samples = value;
                }
            }
            if range.is_none() && audio.is_some() {
                let duration = data.source_duration();
                next.range = RangeMus::new(
                    existing.range.start,
                    existing.range.start + doc.tempo_map().samples_to_mus(duration),
                )?;
            }
        }
    }
    next.validate(doc)?;
    let range = next.range;
    doc.arranger.clips.insert(clip_id, next);
    resolve_overlaps(doc, existing.track_id, clip_id, range, changes)?;
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate()
}

fn move_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    track_id: Option<TrackId>,
    range: RangeMus,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let target_track = track_id.unwrap_or(existing.track_id);
    if target_track != existing.track_id {
        let old_lane = doc.arranger.lanes.get_mut(&existing.track_id);
        if let Some(lane) = old_lane {
            lane.clips.retain(|id| *id != clip_id);
        }
        doc.arranger
            .lanes
            .entry(target_track)
            .or_default()
            .clips
            .push(clip_id);
    }
    let mut next = existing.clone();
    next.track_id = target_track;
    next.range = range;
    next.validate(doc)?;
    doc.arranger.clips.insert(clip_id, next);
    resolve_overlaps(doc, target_track, clip_id, range, changes)?;
    changes.affected.clips.insert(clip_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    doc.validate()
}

fn split_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    at: TimeMus,
    new_clip_id: ClipId,
    new_note_ids: &[NoteId],
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&new_clip_id) {
        return Err(CoreError::DuplicateId {
            id: new_clip_id.to_string(),
        });
    }
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    if at <= existing.range.start || at >= existing.range.end {
        return Err(CoreError::ValidationFailed {
            reason: "split position must lie strictly inside the clip".into(),
        });
    }
    let split_offset = at - existing.range.start;
    let split_samples =
        u64::try_from(doc.tempo_map().samples_delta(split_offset).max(0)).unwrap_or(0);
    let mut right = existing.clone();
    right.id = new_clip_id;
    right.range = RangeMus::new(at, existing.range.end)?;

    match (
        &mut doc.arranger.clips.get_mut(&clip_id).expect("clip").payload,
        &existing.payload,
    ) {
        (ClipPayload::Midi(left_midi), ClipPayload::Midi(orig)) => {
            let split_content = orig.content_start + split_offset;
            let mut left_notes = Vec::new();
            let mut right_notes = Vec::new();
            let mut new_ids = new_note_ids.iter().copied();
            for note in &orig.notes {
                let end = note.start + note.length;
                if end <= split_content {
                    left_notes.push(note.clone());
                } else if note.start >= split_content {
                    let mut moved = note.clone();
                    moved.start = note.start - split_content;
                    right_notes.push(moved);
                } else {
                    let new_id = new_ids.next().ok_or_else(|| CoreError::ValidationFailed {
                        reason: "split requires one new note id per crossing note".into(),
                    })?;
                    if orig.notes.iter().any(|n| n.id == new_id) {
                        return Err(CoreError::DuplicateId {
                            id: new_id.to_string(),
                        });
                    }
                    let mut left = note.clone();
                    left.length = split_content - note.start;
                    left_notes.push(left);
                    right_notes.push(NoteEntity {
                        id: new_id,
                        pitch: note.pitch,
                        start: TimeMus::zero(),
                        length: end - split_content,
                        velocity: note.velocity,
                        release_velocity: note.release_velocity,
                        expression: note.expression.clone(),
                    });
                }
            }
            if new_ids.next().is_some() {
                return Err(CoreError::ValidationFailed {
                    reason: "too many new note ids supplied for split".into(),
                });
            }
            left_midi.notes = left_notes;
            left_midi.length = split_content;
            let right_data = MidiClipData {
                length: orig.length - split_content,
                content_start: TimeMus::zero(),
                notes: right_notes,
            };
            right.payload = ClipPayload::Midi(right_data);
        }
        (ClipPayload::Audio(left_audio), ClipPayload::Audio(orig)) => {
            let split_sample = orig.source_start_sample.saturating_add(split_samples);
            left_audio.source_end_sample = split_sample;
            let mut right_audio = orig.clone();
            right_audio.source_start_sample = split_sample;
            right_audio.fade_in_samples = 0;
            left_audio.fade_out_samples = 0;
            right.payload = ClipPayload::Audio(right_audio);
        }
        _ => unreachable!("validated clip kind"),
    }

    right.validate(doc)?;
    doc.arranger
        .clips
        .get_mut(&clip_id)
        .expect("clip")
        .range
        .end = at;
    doc.arranger.clips.insert(new_clip_id, right);
    doc.arranger
        .lanes
        .entry(existing.track_id)
        .or_default()
        .clips
        .push(new_clip_id);
    if let Some(lane) = doc.arranger.lanes.get_mut(&existing.track_id) {
        sort_lane(lane, &doc.arranger.clips);
    }
    changes.affected.clips.insert(clip_id);
    changes.affected.clips.insert(new_clip_id);
    for id in new_note_ids {
        changes.affected.notes.insert(*id);
    }
    changes.push(DocChange::ClipUpserted(clip_id));
    changes.push(DocChange::ClipUpserted(new_clip_id));
    doc.validate()
}

fn duplicate_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    new_clip_id: ClipId,
    note_id_map: &[(NoteId, NoteId)],
    track_id: Option<TrackId>,
    range: Option<RangeMus>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if doc.arranger.clips.contains_key(&new_clip_id) {
        return Err(CoreError::DuplicateId {
            id: new_clip_id.to_string(),
        });
    }
    let existing = doc
        .arranger
        .clips
        .get(&clip_id)
        .cloned()
        .ok_or_else(|| not_found(&clip_id))?;
    let mut next = existing.clone();
    next.id = new_clip_id;
    next.track_id = track_id.unwrap_or(existing.track_id);
    next.range = range.unwrap_or_else(|| {
        RangeMus::new(
            existing.range.end,
            existing.range.end + existing.range.length(),
        )
        .expect("valid duplicated range")
    });
    if let ClipPayload::Midi(midi) = &mut next.payload {
        let map: std::collections::BTreeMap<NoteId, NoteId> = note_id_map.iter().copied().collect();
        for note in &mut midi.notes {
            let new_id = map
                .get(&note.id)
                .copied()
                .ok_or_else(|| CoreError::ValidationFailed {
                    reason: "duplicate clip requires an id for every note".into(),
                })?;
            note.id = new_id;
        }
    }
    next.validate(doc)?;
    doc.arranger.clips.insert(new_clip_id, next.clone());
    doc.arranger
        .lanes
        .entry(next.track_id)
        .or_default()
        .clips
        .push(new_clip_id);
    resolve_overlaps(doc, next.track_id, new_clip_id, next.range, changes)?;
    changes.affected.clips.insert(new_clip_id);
    changes.push(DocChange::ClipUpserted(new_clip_id));
    doc.validate()
}

fn delete_clip(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if !doc.arranger.clips.contains_key(&clip_id) {
        return Err(not_found(&clip_id));
    }
    remove_clip_internal(doc, clip_id, changes);
    doc.validate()
}

fn set_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note: NoteEntity,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    let ClipPayload::Midi(midi) = &mut clip.payload else {
        return Err(CoreError::ValidationFailed {
            reason: "notes can only be edited in MIDI clips".into(),
        });
    };
    if let Some(existing) = midi.notes.iter_mut().find(|n| n.id == note.id) {
        *existing = note.clone();
    } else {
        midi.notes.push(note.clone());
    }
    midi.validate()?;
    changes.affected.clips.insert(clip_id);
    changes.affected.notes.insert(note.id);
    changes.push(DocChange::ClipUpserted(clip_id));
    Ok(())
}

fn delete_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note_id: NoteId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    let ClipPayload::Midi(midi) = &mut clip.payload else {
        return Err(CoreError::ValidationFailed {
            reason: "notes can only be edited in MIDI clips".into(),
        });
    };
    let before = midi.notes.len();
    midi.notes.retain(|note| note.id != note_id);
    if midi.notes.len() == before {
        return Err(not_found(&note_id));
    }
    changes.affected.clips.insert(clip_id);
    changes.affected.notes.insert(note_id);
    changes.push(DocChange::ClipUpserted(clip_id));
    Ok(())
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
