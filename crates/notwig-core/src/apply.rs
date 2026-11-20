use std::collections::BTreeSet;
use std::sync::Arc;

use crate::CoreError;
use crate::domain::{DeviceChain, DeviceInstance, DeviceKind, ProjectDoc, SoloContext};
use crate::ids::{ClipId, DeviceChainId, DeviceId, NoteId, SliceId, TrackId};
use crate::protocol::{
    AffectedSet, AudioEdit, Command, CommandPayload, DocChange, EngineMsg, PlaybackSchedule,
    StateDelta,
};
use crate::sequencer::{
    AudioClipData, AudioSlice, ClipEntity, ClipKind, ClipPayload, Lane, MidiClipData, NoteEntity,
    sort_lane,
};
use crate::time::{RangeMus, TimeMus};
use crate::tracks::{TrackKind, TrackOutput};
use uuid::Uuid;

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
    let engine_msgs = apply_payload(&mut next, &command.payload, &mut changes)?;
    let rev = next.allocate_revision();
    next.meta.modified_utc = next.meta.modified_utc.max(next.meta.created_utc);
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
            metronome,
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
            if let Some(value) = metronome {
                doc.transport.metronome_enabled = *value;
            }
            doc.transport.validate()?;
            changes.push(DocChange::Transport(doc.transport.clone()));
            Ok(vec![transport_msg(doc)])
        }
        CommandPayload::SetTempo { tempo_bpm } => {
            let previous_bpm = doc.meta.tempo_bpm;
            doc.meta.tempo_bpm = *tempo_bpm;
            if (previous_bpm - tempo_bpm).abs() > f64::EPSILON {
                rescale_unsliced_audio_clips(doc, changes);
            }
            doc.validate()?;
            changes.push(DocChange::Meta(doc.meta.clone()));
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetTimeSignature {
            numerator,
            denominator,
        } => {
            doc.meta.time_sig_num = *numerator;
            doc.meta.time_sig_den = *denominator;
            doc.validate()?;
            changes.push(DocChange::Meta(doc.meta.clone()));
            Ok(Vec::new())
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
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::RenameTrack { id, name } => {
            if name.trim().is_empty() {
                return Err(CoreError::ValidationFailed {
                    reason: "track name must not be empty".into(),
                });
            }
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.name.clone_from(name);
            changes.track(doc, *id);
            Ok(Vec::new())
        }
        CommandPayload::MoveTrack { id, parent, index } => {
            move_track(doc, *id, *parent, *index)?;
            changes.track(doc, *id);
            changes.push(DocChange::TrackOrder(doc.track_order.clone()));
            Ok(Vec::new())
        }
        CommandPayload::SetTrackColor { id, color } => {
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.color = *color;
            changes.track(doc, *id);
            Ok(Vec::new())
        }
        CommandPayload::SetTrackComment { id, comment } => {
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.comment.clone_from(comment);
            changes.track(doc, *id);
            Ok(Vec::new())
        }
        CommandPayload::SetTrackActive { id, active } => {
            let track = doc.tracks.get_mut(id).ok_or_else(|| not_found(id))?;
            track.active = *active;
            changes.track(doc, *id);
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
        CommandPayload::SetClipColor { clip_id, color } => {
            let clip = doc
                .arranger
                .clips
                .get_mut(clip_id)
                .ok_or_else(|| not_found(clip_id))?;
            clip.color = *color;
            changes.affected.clips.insert(*clip_id);
            changes.push(DocChange::ClipUpserted(*clip_id));
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetClipRange { clip_id, range } => {
            let existing = doc
                .arranger
                .clips
                .get(clip_id)
                .cloned()
                .ok_or_else(|| not_found(clip_id))?;
            if range.is_empty() || range.start < TimeMus::zero() {
                return Err(CoreError::ValidationFailed {
                    reason: "clip range must be non-empty and start at or after zero".into(),
                });
            }
            let mut clip = existing.clone();
            if let ClipPayload::Midi(midi) = &mut clip.payload {
                resize_midi_clip_start(midi, range.start - existing.range.start);
            }
            clip.range = *range;
            resize_audio_clip_range(doc, &existing, &mut clip)?;
            clip.validate(doc)?;
            clear_range(doc, existing.track_id, *range, Some(*clip_id), changes)?;
            doc.arranger.clips.insert(*clip_id, clip);
            changes.affected.clips.insert(*clip_id);
            changes.push(DocChange::ClipUpserted(*clip_id));
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::ClearRange { track_id, range } => {
            if !doc.tracks.contains_key(track_id) {
                return Err(not_found(track_id));
            }
            clear_range(doc, *track_id, *range, None, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetNote { clip_id, note } => {
            set_note(doc, *clip_id, note.clone(), changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::CreateNote { clip_id, note } => {
            create_note(doc, *clip_id, note.clone(), changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetNotes { clip_id, notes } => {
            set_notes(doc, *clip_id, notes, changes)?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::DeleteNotes { clip_id, note_ids } => {
            delete_notes(doc, *clip_id, note_ids, changes)?;
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
        CommandPayload::SetClipSlices { clip_id, slices } => {
            let clip = doc
                .arranger
                .clips
                .get_mut(clip_id)
                .ok_or_else(|| not_found(clip_id))?;
            let ClipPayload::Audio(audio) = &mut clip.payload else {
                return Err(CoreError::ValidationFailed {
                    reason: "clip is not an audio clip".into(),
                });
            };
            audio.slices.clone_from(slices);
            audio.slices_enabled = true;
            changes.affected.clips.insert(*clip_id);
            changes.push(DocChange::ClipUpserted(*clip_id));
            doc.validate().map_err(|err| CoreError::ValidationFailed {
                reason: err.to_string(),
            })?;
            Ok(vec![schedule_msg(doc)])
        }
        CommandPayload::SetDeviceState {
            device_id,
            state_ref,
        } => {
            let device = doc
                .devices
                .get_mut(device_id)
                .ok_or_else(|| not_found(device_id))?;
            device.state_ref.clone_from(state_ref);
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
        CommandPayload::ReplaceAsset { asset_id, entry } => {
            if entry.id != *asset_id {
                return Err(CoreError::ValidationFailed {
                    reason: "asset id does not match entry id".into(),
                });
            }
            if !doc.assets.assets.contains_key(asset_id) {
                return Err(not_found(asset_id));
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
            entry.uri.clone_from(uri);
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
        metronome: Some(crate::protocol::MetronomeState {
            enabled: doc.transport.metronome_enabled,
            beats_per_bar: doc.meta.time_sig_num,
        }),
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
        comment: String::new(),
        active: true,
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
    clear_range(doc, track_id, range, None, changes)?;
    doc.arranger.clips.insert(id, clip);
    let lane = doc.arranger.lanes.entry(track_id).or_default();
    lane.clips.push(id);
    changes.affected.clips.insert(id);
    changes.push(DocChange::ClipUpserted(id));
    doc.validate()
}

fn audio_slice_window(
    map: crate::time::TempoMap,
    slice: &AudioSlice,
    from: TimeMus,
    to: TimeMus,
    new_origin: TimeMus,
    new_id: bool,
) -> Option<AudioSlice> {
    let duration = map.samples_to_mus(slice.duration_samples());
    let start = slice.start;
    let end = start + duration;
    let window_start = from.max(start);
    let window_end = to.min(end);
    if window_end <= window_start {
        return None;
    }
    let offset_start = u64::try_from(map.samples_delta(window_start - start)).unwrap_or(0);
    let offset_end = u64::try_from(map.samples_delta(window_end - start)).unwrap_or(0);
    let silent = slice.silent_prefix_samples;
    let audio_begin = offset_start.max(silent);
    let silent_prefix_samples = audio_begin - offset_start;
    let max_end = i128::from(slice.source_end_sample);
    let base = i128::from(slice.source_start_sample);
    let source_start = base + i128::from(audio_begin - silent);
    let source_end = if offset_end <= silent {
        source_start
    } else {
        base + i128::from(offset_end - silent)
    };
    let source_start = source_start.clamp(0, max_end);
    let source_end = source_end.clamp(source_start, max_end);
    if source_end <= source_start && silent_prefix_samples == 0 {
        return None;
    }
    let mut segment = slice.clone();
    if new_id {
        segment.id = SliceId::from_uuid(Uuid::new_v4());
    }
    segment.start = window_start - new_origin;
    segment.source_start_sample = u64::try_from(source_start).unwrap_or(0);
    segment.source_end_sample = u64::try_from(source_end).unwrap_or(0);
    segment.silent_prefix_samples = silent_prefix_samples;
    Some(segment)
}

pub(crate) fn audio_slice_window_set(
    map: crate::time::TempoMap,
    audio: &AudioClipData,
    from: TimeMus,
    to: TimeMus,
    new_origin: TimeMus,
    new_ids: bool,
) -> Vec<AudioSlice> {
    audio
        .slices
        .iter()
        .filter_map(|slice| audio_slice_window(map, slice, from, to, new_origin, new_ids))
        .collect()
}

fn clear_range(
    doc: &mut ProjectDoc,
    track_id: TrackId,
    range: RangeMus,
    exclude: Option<ClipId>,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let Some(lane) = doc.arranger.lanes.get(&track_id).cloned() else {
        return Ok(());
    };
    let mut remove = Vec::new();
    for other in lane.clips {
        if exclude == Some(other) {
            continue;
        }
        let Some(existing) = doc.arranger.clips.get(&other).cloned() else {
            continue;
        };
        if !existing.range.intersects(range) {
            continue;
        }
        if existing.range.start >= range.start && existing.range.end <= range.end {
            remove.push(other);
        } else if existing.range.start < range.start && existing.range.end > range.end {
            let right_id = ClipId::from_uuid(Uuid::new_v4());
            let new_note_ids = crossing_note_ids(&existing, range.start);
            split_clip(doc, other, range.start, right_id, &new_note_ids, changes)?;
            trim_clip_start(doc, &doc.arranger.clips[&right_id].clone(), range.end)?;
            changes.affected.clips.insert(right_id);
            changes.push(DocChange::ClipUpserted(right_id));
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

fn crossing_note_ids(clip: &ClipEntity, at: TimeMus) -> Vec<NoteId> {
    let ClipPayload::Midi(midi) = &clip.payload else {
        return Vec::new();
    };
    let split = midi.content_start + (at - clip.range.start);
    midi.notes
        .iter()
        .filter(|note| note.start < split && note.start + note.length > split)
        .map(|_| NoteId::from_uuid(Uuid::new_v4()))
        .collect()
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

/// Keeps audio content anchored while the clip container changes.
///
/// Unsliced clips are sample-locked. Extending the container adds silence and
/// converts the clip to a single slice; shrinking it trims the source window.
/// Sliced clips keep their absolute positions and rely on engine-side clipping.
fn resize_audio_clip_range(
    doc: &ProjectDoc,
    old: &ClipEntity,
    next: &mut ClipEntity,
) -> Result<(), CoreError> {
    let new_range = next.range;
    let ClipPayload::Audio(audio) = &mut next.payload else {
        return Ok(());
    };
    let map = doc.tempo_map();
    if audio.uses_slices() {
        let left_delta = new_range.start - old.range.start;
        for slice in &mut audio.slices {
            slice.start = slice.start - left_delta;
        }
        audio.slices_enabled = true;
        return Ok(());
    }
    let extends = new_range.start < old.range.start || new_range.end > old.range.end;
    let new_samples = map.mus_to_samples(new_range.length());
    if extends || new_samples > audio.source_duration() {
        let relative_start = old.range.start - new_range.start;
        audio.slices = vec![AudioSlice {
            id: SliceId::from_uuid(Uuid::new_v4()),
            start: relative_start,
            source_start_sample: audio.source_start_sample,
            source_end_sample: audio.source_end_sample,
            silent_prefix_samples: 0,
            gain_db: audio.gain_db,
            asset_id: None,
        }];
        audio.slices_enabled = true;
        return Ok(());
    }
    let left = map.samples_delta(new_range.start - old.range.start);
    let right = map.samples_delta(old.range.end - new_range.end);
    let start = i128::from(audio.source_start_sample) + i128::from(left);
    let end = i128::from(audio.source_end_sample) - i128::from(right);
    if start < 0 || end <= start {
        return Err(CoreError::ValidationFailed {
            reason: "audio trim exceeds source material".into(),
        });
    }
    audio.source_start_sample = u64::try_from(start).unwrap_or(0);
    audio.source_end_sample = u64::try_from(end).unwrap_or(0);
    Ok(())
}

/// Unsliced audio stays sample-locked across tempo changes: the musical range
/// is rescaled so the same source sample count still fits the clip.
fn rescale_unsliced_audio_clips(doc: &mut ProjectDoc, changes: &mut Changes) {
    let map = doc.tempo_map();
    let ids: Vec<ClipId> = doc.arranger.clips.keys().copied().collect();
    for id in ids {
        let Some(clip) = doc.arranger.clips.get_mut(&id) else {
            continue;
        };
        let ClipPayload::Audio(audio) = &clip.payload else {
            continue;
        };
        if audio.uses_slices() {
            continue;
        }
        let length = map.samples_to_mus(audio.source_duration());
        let Ok(range) = RangeMus::new(clip.range.start, clip.range.start + length) else {
            continue;
        };
        if range == clip.range {
            continue;
        }
        clip.range = range;
        changes.affected.clips.insert(id);
        changes.push(DocChange::ClipUpserted(id));
    }
}

fn resize_midi_clip_start(midi: &mut crate::sequencer::MidiClipData, delta: TimeMus) {
    if delta == TimeMus::zero() {
        return;
    }
    let desired = midi.content_start + delta;
    if desired >= TimeMus::zero() {
        midi.content_start = desired;
        return;
    }
    let shift = TimeMus::zero() - desired;
    for note in &mut midi.notes {
        note.start = note.start + shift;
    }
    midi.length = midi.length + shift;
    midi.content_start = TimeMus::zero();
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
            resize_midi_clip_start(midi, delta);
        }
        ClipPayload::Audio(audio) => {
            if audio.uses_slices() {
                let map = doc.tempo_map();
                audio.slices = audio_slice_window_set(
                    map,
                    audio,
                    delta,
                    delta + next.range.length(),
                    delta,
                    false,
                );
                audio.slices_enabled = true;
            } else {
                let samples = doc.tempo_map().samples_delta(delta);
                audio.source_start_sample = audio
                    .source_start_sample
                    .saturating_add(u64::try_from(samples.max(0)).unwrap_or(0));
            }
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
            if audio.uses_slices() {
                let map = doc.tempo_map();
                audio.slices = audio_slice_window_set(
                    map,
                    audio,
                    TimeMus::zero(),
                    next.range.length(),
                    TimeMus::zero(),
                    false,
                );
                audio.slices_enabled = true;
            } else {
                let delta = clip.range.end - new_end;
                let samples = doc.tempo_map().samples_delta(delta);
                audio.source_end_sample = audio
                    .source_end_sample
                    .saturating_sub(u64::try_from(samples.max(0)).unwrap_or(0));
            }
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
        if let ClipPayload::Midi(midi) = &mut next.payload {
            resize_midi_clip_start(midi, new_range.start - existing.range.start);
        }
        if let ClipPayload::Audio(data) = &mut next.payload {
            let old = existing.range;
            if data.uses_slices() {
                let map = doc.tempo_map();
                let left_trim = new_range.start - old.start;
                data.slices = audio_slice_window_set(
                    map,
                    data,
                    left_trim,
                    left_trim + new_range.length(),
                    left_trim,
                    false,
                );
                data.slices_enabled = true;
            } else {
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
    clear_range(doc, existing.track_id, range, Some(clip_id), changes)?;
    doc.arranger.clips.insert(clip_id, next);
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
    if target_track != existing.track_id
        && let Some(lane) = doc.arranger.lanes.get_mut(&existing.track_id)
    {
        lane.clips.retain(|id| *id != clip_id);
    }
    let mut next = existing.clone();
    next.track_id = target_track;
    next.range = range;
    next.validate(doc)?;
    clear_range(doc, target_track, range, Some(clip_id), changes)?;
    if target_track != existing.track_id {
        doc.arranger
            .lanes
            .entry(target_track)
            .or_default()
            .clips
            .push(clip_id);
    }
    doc.arranger.clips.insert(clip_id, next);
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
    let map = doc.tempo_map();
    let split_samples = u64::try_from(map.samples_delta(split_offset).max(0)).unwrap_or(0);
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
            if orig.uses_slices() {
                left_audio.slices = audio_slice_window_set(
                    map,
                    orig,
                    TimeMus::zero(),
                    split_offset,
                    TimeMus::zero(),
                    false,
                );
                left_audio.slices_enabled = true;
                let mut right_audio = orig.clone();
                right_audio.slices = audio_slice_window_set(
                    map,
                    orig,
                    split_offset,
                    existing.range.length(),
                    split_offset,
                    true,
                );
                right_audio.slices_enabled = true;
                right.payload = ClipPayload::Audio(right_audio);
            } else {
                let split_sample = orig.source_start_sample.saturating_add(split_samples);
                left_audio.source_end_sample = split_sample;
                let mut right_audio = orig.clone();
                right_audio.source_start_sample = split_sample;
                right_audio.fade_in_samples = 0;
                left_audio.fade_out_samples = 0;
                right.payload = ClipPayload::Audio(right_audio);
            }
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
    clear_range(doc, next.track_id, next.range, None, changes)?;
    doc.arranger.clips.insert(new_clip_id, next.clone());
    doc.arranger
        .lanes
        .entry(next.track_id)
        .or_default()
        .clips
        .push(new_clip_id);
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

fn midi_clip_mut(doc: &mut ProjectDoc, clip_id: ClipId) -> Result<&mut MidiClipData, CoreError> {
    let clip = doc
        .arranger
        .clips
        .get_mut(&clip_id)
        .ok_or_else(|| not_found(&clip_id))?;
    match &mut clip.payload {
        ClipPayload::Midi(midi) => Ok(midi),
        ClipPayload::Audio(_) => Err(CoreError::ValidationFailed {
            reason: "notes can only be edited in MIDI clips".into(),
        }),
    }
}

fn sort_notes(notes: &mut [NoteEntity]) {
    notes.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(a.pitch.cmp(&b.pitch))
            .then(a.id.cmp(&b.id))
    });
}

fn record_note_change(changes: &mut Changes, clip_id: ClipId, note_ids: &[NoteId]) {
    changes.affected.clips.insert(clip_id);
    for id in note_ids {
        changes.affected.notes.insert(*id);
    }
    changes.push(DocChange::ClipUpserted(clip_id));
}

fn set_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note: NoteEntity,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let note_id = note.id;
    let midi = midi_clip_mut(doc, clip_id)?;
    if let Some(existing) = midi.notes.iter_mut().find(|n| n.id == note.id) {
        *existing = note;
    } else {
        midi.notes.push(note);
    }
    sort_notes(&mut midi.notes);
    midi.validate()?;
    record_note_change(changes, clip_id, &[note_id]);
    Ok(())
}

fn create_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note: NoteEntity,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let note_id = note.id;
    let midi = midi_clip_mut(doc, clip_id)?;
    if midi.notes.iter().any(|existing| existing.id == note_id) {
        return Err(CoreError::DuplicateId {
            id: note_id.to_string(),
        });
    }
    note.validate()?;
    midi.notes.push(note);
    sort_notes(&mut midi.notes);
    midi.validate()?;
    record_note_change(changes, clip_id, &[note_id]);
    Ok(())
}

fn set_notes(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    notes: &[NoteEntity],
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let mut ids = BTreeSet::new();
    for note in notes {
        if !ids.insert(note.id) {
            return Err(CoreError::DuplicateId {
                id: note.id.to_string(),
            });
        }
    }
    let midi = midi_clip_mut(doc, clip_id)?;
    for note in notes {
        if let Some(existing) = midi.notes.iter_mut().find(|n| n.id == note.id) {
            *existing = note.clone();
        } else {
            midi.notes.push(note.clone());
        }
    }
    sort_notes(&mut midi.notes);
    midi.validate()?;
    let ids: Vec<NoteId> = ids.into_iter().collect();
    record_note_change(changes, clip_id, &ids);
    Ok(())
}

fn delete_notes(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note_ids: &[NoteId],
    changes: &mut Changes,
) -> Result<(), CoreError> {
    if note_ids.is_empty() {
        return Ok(());
    }
    let requested: BTreeSet<NoteId> = note_ids.iter().copied().collect();
    let midi = midi_clip_mut(doc, clip_id)?;
    for id in &requested {
        if !midi.notes.iter().any(|note| note.id == *id) {
            return Err(not_found(id));
        }
    }
    midi.notes.retain(|note| !requested.contains(&note.id));
    midi.validate()?;
    let ids: Vec<NoteId> = requested.into_iter().collect();
    record_note_change(changes, clip_id, &ids);
    Ok(())
}

fn delete_note(
    doc: &mut ProjectDoc,
    clip_id: ClipId,
    note_id: NoteId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    delete_notes(doc, clip_id, &[note_id], changes)
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
