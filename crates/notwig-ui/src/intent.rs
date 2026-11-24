use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::{
    AssetId, ClipId, Color, Db, DeviceChainId, DeviceId, DeviceKind, NoteEntity, NoteId, Pan,
    ParamId, ParamValue, RangeMus, TimeMus, TrackId, TrackKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Wav,
    Aiff,
    Flac,
    Mp3,
    Ogg,
}

impl ExportFormat {
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportBitDepth {
    Int16,
    Int24,
    #[default]
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub bit_depth: ExportBitDepth,
    pub dither: bool,
    pub normalize: bool,
    pub stems: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    Play,
    Pause,
    Stop,
    ToggleRecord,
    ToggleLoop,
    ToggleMetronome,
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
    SetClipColor {
        clip_id: ClipId,
        color: Option<Color>,
    },
    RenameClip {
        clip_id: ClipId,
        name: String,
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
    SetTrackRecordArm {
        track_id: TrackId,
        armed: bool,
    },
    SetTrackInput {
        track_id: TrackId,
        input: Option<notwig_core::AudioInputRef>,
    },
    SetTrackMonitorMode {
        track_id: TrackId,
        mode: notwig_core::MonitorMode,
    },
    GroupTracks {
        tracks: Vec<TrackId>,
    },
    JoinClips {
        clips: Vec<ClipId>,
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
    InsertDevice {
        track_id: TrackId,
        chain_id: DeviceChainId,
        type_id: String,
        version: notwig_core::SchemaVersion,
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
    AnalyzeAudioDuration(PathBuf),
    DropAudio {
        path: PathBuf,
        target: DropAudioTarget,
        at: TimeMus,
    },
    DropAudioBytes {
        name: String,
        bytes: Arc<[u8]>,
        target: DropAudioTarget,
        at: TimeMus,
    },
    InsertAudioClip {
        track_id: TrackId,
        asset_id: AssetId,
    },
    RemoveDevice(DeviceId),
    MoveDevice {
        device_id: DeviceId,
        to_chain: DeviceChainId,
        index: usize,
    },
    CopyDevices(Vec<DeviceId>),
    CutDevices(Vec<DeviceId>),
    PasteDevices {
        track_id: TrackId,
        chain_id: DeviceChainId,
        index: usize,
    },
    SetDeviceBypass {
        device_id: DeviceId,
        bypass: bool,
    },
    SetDevicesActive {
        device_ids: Vec<DeviceId>,
        active: bool,
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
    CloseProject,
    OpenProject,
    OpenProjectFolder,
    OpenProjectNamed(String),
    Save,
    SaveAs,
    SaveProjectNamed(String),
    CancelProjectPicker,
    DownloadProject,
    RenderMixdownRange {
        start: TimeMus,
        end: TimeMus,
        options: ExportOptions,
    },
    Undo,
    Redo,
    CancelRender,
    RestartAudioEngine,
    StopAudioEngine,
    SetExtensionEnabled {
        id: String,
        enabled: bool,
    },
    ReloadExtensions,
    SetExtensionIndexes {
        indexes: Vec<String>,
    },
    RefreshExtensionIndex,
    InstallExtensionFromIndex {
        id: String,
    },
    InstallExtensionZipBytes {
        name: String,
        bytes: Arc<[u8]>,
    },
    InstallExtensionZip {
        path: PathBuf,
    },
    ImportExtensionZip,
    UninstallExtension {
        id: String,
    },
    ClearNotifications,
    ApplyAudioSettings {
        driver: String,
        device: String,
        sample_rate: Option<u32>,
        block_size: u32,
        low_latency_block_size: u32,
        low_latency_monitoring: bool,
        channels: Vec<crate::runtime::AudioChannelConfig>,
        master_output: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAudioTarget {
    Track(TrackId),
    NewTrack { index: usize },
    ClipEditor { clip_id: ClipId },
}
