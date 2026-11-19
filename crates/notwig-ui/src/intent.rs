use std::path::PathBuf;

use notwig_core::{
    AssetId, ClipId, Color, Db, DeviceChainId, DeviceId, DeviceKind, NoteEntity, NoteId, Pan,
    ParamId, ParamValue, RangeMus, TimeMus, TrackId, TrackKind,
};

#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    Play,
    Pause,
    Stop,
    ToggleLoop,
    SetPlayhead(TimeMus),
    SetTempo(f64),
    SetTimeSignature {
        numerator: u8,
        denominator: u8,
    },
    SetPlayStart(TimeMus),
    SetLoopRange(RangeMus),
    SelectTrack(TrackId),
    SelectClip(ClipId),
    SelectDevice(DeviceId),
    CreateTrack(TrackKind),
    DeleteTrack(TrackId),
    DuplicateTrack(TrackId),
    DeleteClip(ClipId),
    CreateMidiClip {
        track_id: TrackId,
        range: RangeMus,
    },
    CreateAudioClip {
        track_id: TrackId,
        range: RangeMus,
    },
    EditClipRange {
        clip_id: ClipId,
        range: RangeMus,
    },
    SetClipRange {
        clip_id: ClipId,
        range: RangeMus,
    },
    MoveClip {
        clip_id: ClipId,
        track_id: TrackId,
        range: RangeMus,
    },
    MoveClips {
        clips: Vec<(ClipId, TrackId, RangeMus)>,
    },
    SetClipMuted {
        clip_id: ClipId,
        muted: bool,
    },
    DuplicateClip {
        clip_id: ClipId,
    },
    CopySelection {
        clips: Vec<ClipId>,
        time: Option<(RangeMus, Vec<TrackId>)>,
    },
    CutSelection {
        clips: Vec<ClipId>,
        time: Option<(RangeMus, Vec<TrackId>)>,
    },
    PasteClips {
        at: TimeMus,
    },
    SetClipboard(crate::Clipboard),
    ClipboardError(String),
    DeleteSelection {
        clips: Vec<ClipId>,
        time: Option<(RangeMus, Vec<TrackId>)>,
    },
    DuplicateSelection {
        clips: Vec<ClipId>,
        time: Option<(RangeMus, Vec<TrackId>)>,
    },
    MoveTimeSelection {
        range: RangeMus,
        tracks: Vec<TrackId>,
        delta: TimeMus,
    },
    MoveTrack {
        id: TrackId,
        parent: Option<TrackId>,
        index: usize,
    },
    SetTrackColor {
        track_id: TrackId,
        color: Option<Color>,
    },
    SetTrackComment {
        track_id: TrackId,
        comment: String,
    },
    RenameTrack {
        track_id: TrackId,
        name: String,
    },
    SetTrackActive {
        track_id: TrackId,
        active: bool,
    },
    GroupTracks {
        tracks: Vec<TrackId>,
    },
    SplitClip {
        clip_id: ClipId,
        at: TimeMus,
    },
    SetClipSlices {
        clip_id: ClipId,
        slices: Vec<notwig_core::sequencer::AudioSlice>,
    },
    CreateMidiNote {
        clip_id: ClipId,
        note: NoteEntity,
    },
    EditMidiNotes {
        clip_id: ClipId,
        notes: Vec<NoteEntity>,
        removed: Vec<NoteId>,
    },
    PreviewNote {
        track_id: TrackId,
        note_id: NoteId,
        pitch: u8,
        velocity: f32,
        length_mus: Option<TimeMus>,
    },
    ReleasePreviewNote {
        track_id: TrackId,
        note_id: NoteId,
    },
    SetTrackMixer {
        track_id: TrackId,
        volume_db: Option<Db>,
        pan: Option<Pan>,
        mute: Option<bool>,
        solo: Option<bool>,
    },
    InsertStockDevice {
        track_id: TrackId,
        chain_id: DeviceChainId,
        type_id: String,
        kind: DeviceKind,
        index: Option<usize>,
    },
    InsertPlugin {
        track_id: TrackId,
        chain_id: DeviceChainId,
        type_id: String,
        kind: DeviceKind,
        index: Option<usize>,
    },
    ImportAudio,
    DropAudio {
        path: PathBuf,
        target: DropAudioTarget,
        at: TimeMus,
    },
    InsertAudioClip {
        track_id: TrackId,
        asset_id: AssetId,
    },
    RemoveDevice(DeviceId),
    SetDeviceBypass {
        device_id: DeviceId,
        bypass: bool,
    },
    OpenPluginWindow {
        device_id: DeviceId,
    },
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: ParamValue,
    },
    RelinkAsset(AssetId),
    Quit,
    NewProject,
    NewProjectAtRate(u32),
    OpenProject,
    Save,
    SaveAs,
    RenderMixdownRange {
        start: TimeMus,
        end: TimeMus,
    },
    Undo,
    Redo,
    ClearNotifications,
    ApplyAudioSettings {
        driver: String,
        device: String,
        sample_rate: Option<u32>,
        block_size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAudioTarget {
    Track(TrackId),
    NewTrack { index: usize },
    ClipEditor { clip_id: ClipId },
}
