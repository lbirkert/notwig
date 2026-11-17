use std::path::PathBuf;

use notwig_core::{
    AssetId, ClipId, Db, DeviceChainId, DeviceId, DeviceKind, Pan, ParamId, ParamValue, RangeMus,
    TimeMus, TrackId, TrackKind,
};

#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    Play,
    Pause,
    Stop,
    ToggleLoop,
    SetPlayhead(TimeMus),
    SetLoopRange(RangeMus),
    SelectTrack(TrackId),
    SelectClip(ClipId),
    SelectDevice(DeviceId),
    CreateTrack(TrackKind),
    DeleteTrack(TrackId),
    DeleteClip(ClipId),
    CreateMidiClip {
        track_id: TrackId,
        range: RangeMus,
    },
    EditClipRange {
        clip_id: ClipId,
        range: RangeMus,
    },
    MoveClip {
        clip_id: ClipId,
        track_id: TrackId,
        range: RangeMus,
    },
    SetClipMuted {
        clip_id: ClipId,
        muted: bool,
    },
    DuplicateClip {
        clip_id: ClipId,
    },
    SplitClip {
        clip_id: ClipId,
        at: TimeMus,
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
    },
    InsertPlugin {
        track_id: TrackId,
        chain_id: DeviceChainId,
        type_id: String,
        kind: DeviceKind,
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
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: ParamValue,
    },
    RelinkAsset(AssetId),
    NewProject,
    OpenProject,
    Save,
    SaveAs,
    RenderMixdown,
    ClearNotifications,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAudioTarget {
    Track(TrackId),
    NewTrack { index: usize },
}
