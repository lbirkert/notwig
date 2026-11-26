use std::sync::Arc;

use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{DeviceId, TrackId};
use crate::protocol::{
    AffectedSet, Command, CommandPayload, DocChange, EngineMsg, PlaybackSchedule, StateDelta,
};

use super::{ApplyResult, assets, clips, devices, notes, tracks, transport};

#[derive(Default)]
pub(super) struct Changes {
    pub(super) changes: Vec<DocChange>,
    pub(super) affected: AffectedSet,
}

impl Changes {
    pub(super) fn track(&mut self, id: TrackId) {
        self.affected.tracks.insert(id);
        self.changes.push(DocChange::TrackUpserted(id));
    }

    pub(super) fn device(&mut self, id: DeviceId) {
        self.affected.devices.insert(id);
        self.changes.push(DocChange::DeviceUpserted(id));
    }

    pub(super) fn push(&mut self, change: DocChange) {
        self.changes.push(change);
    }
}

pub fn apply(doc: &ProjectDoc, command: &Command) -> Result<ApplyResult, CoreError> {
    let mut next = doc.clone();
    let mut changes = Changes::default();
    let engine_msgs = apply_payload(&mut next, &command.payload, &mut changes)?;
    // The apply boundary is the single place that guarantees the returned
    // document is internally consistent. Commands may perform narrower checks
    // while mutating, but no command may hand back a document that fails the
    // document-wide contracts.
    next.validate()?;
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
        } => transport::set_transport(
            doc,
            *playing,
            *playhead,
            *loop_enabled,
            *loop_range,
            *metronome,
            changes,
        ),
        CommandPayload::SetTempo { tempo_bpm } => transport::set_tempo(doc, *tempo_bpm, changes),
        CommandPayload::SetTimeSignature {
            numerator,
            denominator,
        } => transport::set_time_signature(doc, *numerator, *denominator, changes),
        CommandPayload::Pause => Ok(transport::pause(doc, changes)),
        CommandPayload::Stop => Ok(transport::stop(doc, changes)),
        CommandPayload::CreateTrack {
            id,
            chain_id,
            kind,
            name,
            parent,
            index,
            color,
        } => tracks::create_track_command(
            doc,
            *id,
            *chain_id,
            *kind,
            name.clone(),
            *parent,
            *index,
            *color,
            changes,
        ),
        CommandPayload::DeleteTrack { id } => tracks::delete_track_command(doc, *id, changes),
        CommandPayload::RenameTrack { id, name } => tracks::rename_track(doc, *id, name, changes),
        CommandPayload::MoveTrack { id, parent, index } => {
            tracks::move_track_command(doc, *id, *parent, *index, changes)
        }
        CommandPayload::SetTrackColor { id, color } => {
            tracks::set_track_color(doc, *id, *color, changes)
        }
        CommandPayload::SetTrackComment { id, comment } => {
            tracks::set_track_comment(doc, *id, comment, changes)
        }
        CommandPayload::SetTrackActive { id, active } => {
            tracks::set_track_active(doc, *id, *active, changes)
        }
        CommandPayload::SetTrackRecordArm { id, armed } => {
            tracks::set_track_record_arm(doc, *id, *armed, changes)
        }
        CommandPayload::SetTrackInput { id, input } => {
            tracks::set_track_input(doc, *id, input.as_ref(), changes)
        }
        CommandPayload::SetTrackMonitorMode { id, mode } => {
            tracks::set_track_monitor_mode(doc, *id, *mode, changes)
        }
        CommandPayload::SetRecording { recording } => {
            Ok(transport::set_recording(doc, *recording, changes))
        }
        CommandPayload::SetTrackMixer {
            id,
            volume_db,
            pan,
            mute,
            solo,
        } => tracks::set_track_mixer(doc, *id, *volume_db, *pan, *mute, *solo, changes),
        CommandPayload::SetTrackRoute { id, audio_out } => {
            tracks::set_track_route(doc, *id, *audio_out, changes)
        }
        CommandPayload::SetSend { track_id, send } => {
            tracks::set_send(doc, *track_id, send, changes)
        }
        CommandPayload::RemoveSend { track_id, send_id } => {
            tracks::remove_send(doc, *track_id, *send_id, changes)
        }
        CommandPayload::CreateClip {
            id,
            track_id,
            kind,
            range,
            payload,
        } => {
            clips::create_clip_command(doc, *id, *track_id, *kind, *range, payload.clone(), changes)
        }
        CommandPayload::EditClip {
            clip_id,
            range,
            name,
            muted,
            content_start,
            content_length,
            audio,
        } => clips::edit_clip_command(
            doc,
            *clip_id,
            *range,
            name.clone(),
            *muted,
            *content_start,
            *content_length,
            audio.as_ref(),
            changes,
        ),
        CommandPayload::MoveClip {
            clip_id,
            track_id,
            range,
        } => clips::move_clip_command(doc, *clip_id, *track_id, *range, changes),
        CommandPayload::SplitClip {
            clip_id,
            at,
            new_clip_id,
            new_note_ids,
        } => clips::split_clip_command(doc, *clip_id, *at, *new_clip_id, new_note_ids, changes),
        CommandPayload::DuplicateClip {
            clip_id,
            new_clip_id,
            note_id_map,
            track_id,
            range,
        } => clips::duplicate_clip_command(
            doc,
            *clip_id,
            *new_clip_id,
            note_id_map,
            *track_id,
            *range,
            changes,
        ),
        CommandPayload::DeleteClip { clip_id } => {
            clips::delete_clip_command(doc, *clip_id, changes)
        }
        CommandPayload::SetClipColor { clip_id, color } => {
            clips::set_clip_color(doc, *clip_id, *color, changes)
        }
        CommandPayload::SetClipRange { clip_id, range } => {
            clips::set_clip_range(doc, *clip_id, *range, changes)
        }
        CommandPayload::ClearRange { track_id, range } => {
            clips::clear_range_command(doc, *track_id, *range, changes)
        }
        CommandPayload::SetNote { clip_id, note } => {
            notes::set_note(doc, *clip_id, note.clone(), changes)
        }
        CommandPayload::CreateNote { clip_id, note } => {
            notes::create_note(doc, *clip_id, note.clone(), changes)
        }
        CommandPayload::SetNotes { clip_id, notes } => {
            notes::set_notes(doc, *clip_id, notes, changes)
        }
        CommandPayload::DeleteNotes { clip_id, note_ids } => {
            notes::delete_notes(doc, *clip_id, note_ids, changes)
        }
        CommandPayload::DeleteNote { clip_id, note_id } => {
            notes::delete_note(doc, *clip_id, *note_id, changes)
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
        } => devices::insert_device_command(
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
        ),
        CommandPayload::RemoveDevice { device_id } => {
            devices::remove_device_command(doc, *device_id, changes)
        }
        CommandPayload::MoveDevice {
            device_id,
            to_chain,
            index,
        } => devices::move_device_command(doc, *device_id, *to_chain, *index, changes),
        CommandPayload::SetDeviceBypass { device_id, bypass } => {
            devices::set_device_bypass(doc, *device_id, *bypass, changes)
        }
        CommandPayload::SetDeviceActive { device_id, active } => {
            devices::set_device_active(doc, *device_id, *active, changes)
        }
        CommandPayload::SetClipSlices { clip_id, slices } => {
            clips::set_clip_slices(doc, *clip_id, slices, changes)
        }
        CommandPayload::SetClipGainCurve { clip_id, curve } => {
            clips::set_clip_gain_curve(doc, *clip_id, curve.clone(), changes)
        }
        CommandPayload::SetClipPanCurve { clip_id, curve } => {
            clips::set_clip_pan_curve(doc, *clip_id, curve.clone(), changes)
        }
        CommandPayload::SetDeviceState {
            device_id,
            state_ref,
        } => devices::set_device_state(doc, *device_id, state_ref.as_ref(), changes),
        CommandPayload::SetParam {
            device_id,
            param_id,
            value,
            ramp,
        } => devices::set_param(doc, *device_id, *param_id, value, *ramp, changes),
        CommandPayload::RegisterAsset { asset_id, entry } => {
            assets::register_asset(doc, *asset_id, entry, changes)
        }
        CommandPayload::ReplaceAsset { asset_id, entry } => {
            assets::replace_asset(doc, *asset_id, entry, changes)
        }
        CommandPayload::RelinkAsset { asset_id, uri } => {
            assets::relink_asset(doc, *asset_id, uri, changes)
        }
    }
}

pub(super) fn not_found<T: std::fmt::Display>(id: &T) -> CoreError {
    CoreError::NotFound { id: id.to_string() }
}

pub(super) fn schedule_msg(doc: &ProjectDoc) -> EngineMsg {
    EngineMsg::UpdateSchedule {
        schedule: Arc::new(PlaybackSchedule::from_doc(doc)),
    }
}
