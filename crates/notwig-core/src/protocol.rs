use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::{
    AssetEntry, AudioAssetBuffer, DeviceInstance, DeviceKind, ParamValue, ProjectDoc,
};
use crate::ids::{AssetId, ClipId, DeviceChainId, DeviceId, NoteId, ParamId, SendId, TrackId};
use crate::sequencer::{ClipEntity, ClipPayload};
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
                | Self::SetDeviceBypass { .. }
                | Self::SetDeviceState { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RampSpec {
    pub duration_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DocChange {
    Snapshot(ProjectDoc),
    Transport(crate::domain::TransportDoc),
    TrackUpserted(TrackId),
    TrackRemoved(TrackId),
    TrackOrder(crate::tracks::TrackForest),
    ClipUpserted(ClipId),
    ClipRemoved(ClipId),
    DeviceUpserted(DeviceId),
    DeviceRemoved(DeviceId),
    ChainUpserted(DeviceChainId),
    ChainRemoved(DeviceChainId),
    AssetUpserted(AssetId),
    Routing(crate::tracks::RoutingGraph),
    Meta(crate::domain::Meta),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AffectedSet {
    pub tracks: BTreeSet<TrackId>,
    pub clips: BTreeSet<ClipId>,
    pub notes: BTreeSet<NoteId>,
    pub devices: BTreeSet<DeviceId>,
    pub chains: BTreeSet<DeviceChainId>,
    pub assets: BTreeSet<AssetId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateDelta {
    pub rev: u64,
    pub changes: Vec<DocChange>,
    pub affected: AffectedSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Query {
    GetProject,
    GetTrack { id: TrackId },
    GetClip { id: ClipId },
    GetDevice { id: DeviceId },
    ListAssets,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryResult {
    Project(ProjectDoc),
    Track(crate::tracks::Track),
    Clip(ClipEntity),
    Device(DeviceInstance),
    Assets(Vec<AssetEntry>),
    NotFound,
}

#[derive(Clone)]
pub struct PlanHandle(Arc<dyn std::any::Any + Send + Sync>);

impl PlanHandle {
    #[must_use]
    pub fn new<T: std::any::Any + Send + Sync>(plan: T) -> Self {
        Self(Arc::new(plan))
    }

    #[must_use]
    pub fn from_arc<T: std::any::Any + Send + Sync>(plan: Arc<T>) -> Self {
        Self(plan)
    }

    #[must_use]
    pub fn downcast_ref<T: std::any::Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }

    #[must_use]
    pub fn as_arc_any(&self) -> Arc<dyn std::any::Any + Send + Sync> {
        Arc::clone(&self.0)
    }
}

impl fmt::Debug for PlanHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlanHandle(<prepared>)")
    }
}

impl PartialEq for PlanHandle {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSchedule {
    pub tracks: std::collections::BTreeMap<TrackId, Vec<ClipEntity>>,
}

impl PlaybackSchedule {
    #[must_use]
    pub fn from_doc(doc: &ProjectDoc) -> Self {
        let mut tracks: std::collections::BTreeMap<TrackId, Vec<ClipEntity>> =
            std::collections::BTreeMap::new();
        for clip in doc.arranger.clips.values() {
            if clip.muted {
                continue;
            }
            tracks.entry(clip.track_id).or_default().push(clip.clone());
        }
        for clips in tracks.values_mut() {
            clips.sort_by(|a, b| {
                a.range
                    .start
                    .cmp(&b.range.start)
                    .then_with(|| a.id.cmp(&b.id))
            });
        }
        Self { tracks }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetronomeState {
    pub enabled: bool,
    pub beats_per_bar: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineMsg {
    ApplyPlan {
        plan: PlanHandle,
    },
    SetTransport {
        playing: bool,
        playhead_samples: u64,
        loop_enabled: bool,
        loop_start_samples: Option<u64>,
        loop_end_samples: Option<u64>,
        stop: bool,
        metronome: Option<MetronomeState>,
    },
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: ParamValue,
        ramp: Option<RampSpec>,
    },
    SetTrackState {
        track_id: TrackId,
        mute: bool,
        solo_effective: bool,
        gain_db: Db,
        pan: Pan,
    },
    UpdateSchedule {
        schedule: Arc<PlaybackSchedule>,
    },
    PrepareAudioAsset {
        asset_id: AssetId,
        buffer: Arc<AudioAssetBuffer>,
    },
    AllNotesOff,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RtEvent {
    TransportTick {
        playhead_samples: u64,
        playhead_mus: TimeMus,
        playing: bool,
    },
    Meters {
        track_id: TrackId,
        peak_l: f32,
        peak_r: f32,
        rms_l: f32,
        rms_r: f32,
    },
    Status {
        xruns: u64,
        cpu_load: Option<f32>,
    },
    LatencyChanged {
        device_id: Option<DeviceId>,
        samples: u32,
    },
    Error {
        scope: String,
        code: String,
        message: String,
    },
}

impl PlaybackSchedule {
    pub fn midi_clips(&self, track: TrackId) -> impl Iterator<Item = &ClipEntity> {
        self.tracks
            .get(&track)
            .into_iter()
            .flatten()
            .filter(|clip| matches!(clip.payload, ClipPayload::Midi(_)))
    }

    pub fn audio_clips(&self, track: TrackId) -> impl Iterator<Item = &ClipEntity> {
        self.tracks
            .get(&track)
            .into_iter()
            .flatten()
            .filter(|clip| matches!(clip.payload, ClipPayload::Audio(_)))
    }
}
