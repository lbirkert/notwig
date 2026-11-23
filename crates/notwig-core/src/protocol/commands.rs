use serde::{Deserialize, Serialize};

use crate::domain::{AssetEntry, DeviceKind, ParamValue};
use crate::ids::{AssetId, ClipId, DeviceChainId, DeviceId, NoteId, ParamId, SendId, TrackId};
use crate::sequencer::ClipPayload;
use crate::time::{Db, Pan, RangeMus, SchemaVersion, TimeMus};

pub type CmdId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: Option<CmdId>,
    pub payload: CommandPayload,
}

impl Command {
    #[must_use]
    pub fn new(payload: CommandPayload) -> Self {
        Self { id: None, payload }
    }

    #[must_use]
    pub fn with_id(id: CmdId, payload: CommandPayload) -> Self {
        Self {
            id: Some(id),
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandPayload {
    SetTransport {
        playing: Option<bool>,
        playhead: Option<TimeMus>,
        loop_enabled: Option<bool>,
        loop_range: Option<Option<RangeMus>>,
        metronome: Option<bool>,
    },
    SetTempo {
        tempo_bpm: f64,
    },
    SetTimeSignature {
        numerator: u8,
        denominator: u8,
    },
    Pause,
    Stop,
    CreateTrack {
        id: TrackId,
        chain_id: DeviceChainId,
        kind: crate::tracks::TrackKind,
        name: String,
        parent: Option<TrackId>,
        index: Option<usize>,
        color: Option<crate::time::Color>,
    },
    DeleteTrack {
        id: TrackId,
    },
    RenameTrack {
        id: TrackId,
        name: String,
    },
    SetTrackColor {
        id: TrackId,
        color: Option<crate::time::Color>,
    },
    SetTrackComment {
        id: TrackId,
        comment: String,
    },
    SetTrackActive {
        id: TrackId,
        active: bool,
    },
    SetTrackRecordArm {
        id: TrackId,
        armed: bool,
    },
    SetTrackInput {
        id: TrackId,
        input: Option<crate::tracks::AudioInputRef>,
    },
    SetTrackMonitorMode {
        id: TrackId,
        mode: crate::tracks::MonitorMode,
    },
    SetRecording {
        recording: bool,
    },
    MoveTrack {
        id: TrackId,
        parent: Option<TrackId>,
        index: usize,
    },
    SetTrackMixer {
        id: TrackId,
        volume_db: Option<Db>,
        pan: Option<Pan>,
        mute: Option<bool>,
        solo: Option<bool>,
    },
    SetTrackRoute {
        id: TrackId,
        audio_out: crate::tracks::TrackOutput,
    },
    SetSend {
        track_id: TrackId,
        send: crate::tracks::Send,
    },
    RemoveSend {
        track_id: TrackId,
        send_id: SendId,
    },
    CreateClip {
        id: ClipId,
        track_id: TrackId,
        kind: crate::sequencer::ClipKind,
        range: RangeMus,
        payload: ClipPayload,
    },
    EditClip {
        clip_id: ClipId,
        range: Option<RangeMus>,
        name: Option<Option<String>>,
        muted: Option<bool>,
        content_start: Option<TimeMus>,
        content_length: Option<TimeMus>,
        audio: Option<AudioEdit>,
    },
    MoveClip {
        clip_id: ClipId,
        track_id: Option<TrackId>,
        range: RangeMus,
    },
    SplitClip {
        clip_id: ClipId,
        at: TimeMus,
        new_clip_id: ClipId,
        new_note_ids: Vec<NoteId>,
    },
    DuplicateClip {
        clip_id: ClipId,
        new_clip_id: ClipId,
        note_id_map: Vec<(NoteId, NoteId)>,
        track_id: Option<TrackId>,
        range: Option<RangeMus>,
    },
    DeleteClip {
        clip_id: ClipId,
    },
    SetClipColor {
        clip_id: ClipId,
        color: Option<crate::time::Color>,
    },
    SetClipRange {
        clip_id: ClipId,
        range: RangeMus,
    },
    ClearRange {
        track_id: TrackId,
        range: RangeMus,
    },
    SetNote {
        clip_id: ClipId,
        note: crate::sequencer::NoteEntity,
    },
    CreateNote {
        clip_id: ClipId,
        note: crate::sequencer::NoteEntity,
    },
    SetNotes {
        clip_id: ClipId,
        notes: Vec<crate::sequencer::NoteEntity>,
    },
    DeleteNotes {
        clip_id: ClipId,
        note_ids: Vec<NoteId>,
    },
    DeleteNote {
        clip_id: ClipId,
        note_id: NoteId,
    },
    InsertDevice {
        id: DeviceId,
        type_id: String,
        version: Option<SchemaVersion>,
        kind: DeviceKind,
        target_chain: DeviceChainId,
        index: Option<usize>,
        child_chain_ids: Vec<DeviceChainId>,
        name: Option<String>,
    },
    RemoveDevice {
        device_id: DeviceId,
    },
    MoveDevice {
        device_id: DeviceId,
        to_chain: DeviceChainId,
        index: usize,
    },
    SetDeviceBypass {
        device_id: DeviceId,
        bypass: bool,
    },
    SetDeviceActive {
        device_id: DeviceId,
        active: bool,
    },
    SetDeviceState {
        device_id: DeviceId,
        state_ref: Option<crate::domain::StateBlobRef>,
    },
    SetClipSlices {
        clip_id: ClipId,
        slices: Vec<crate::sequencer::AudioSlice>,
    },
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: ParamValue,
        ramp: Option<RampSpec>,
    },
    RegisterAsset {
        asset_id: AssetId,
        entry: AssetEntry,
    },
    ReplaceAsset {
        asset_id: AssetId,
        entry: AssetEntry,
    },
    RelinkAsset {
        asset_id: AssetId,
        uri: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEdit {
    pub source_start_sample: Option<u64>,
    pub source_end_sample: Option<u64>,
    pub gain_db: Option<Db>,
    pub fade_in_samples: Option<u64>,
    pub fade_out_samples: Option<u64>,
}

impl CommandPayload {
    #[must_use]
    pub fn requires_plan_rebuild(&self) -> bool {
        matches!(
            self,
            Self::SetTempo { .. }
                | Self::SetTimeSignature { .. }
                | Self::SetTrackActive { .. }
                | Self::CreateTrack { .. }
                | Self::DeleteTrack { .. }
                | Self::MoveTrack { .. }
                | Self::SetTrackRoute { .. }
                | Self::SetSend { .. }
                | Self::RemoveSend { .. }
                | Self::InsertDevice { .. }
                | Self::RemoveDevice { .. }
                | Self::MoveDevice { .. }
                | Self::SetDeviceActive { .. }
                | Self::SetDeviceState { .. }
                | Self::SetTrackMonitorMode { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RampSpec {
    pub duration_ms: f32,
}
