use std::sync::Arc;

use crate::domain::{AudioAssetBuffer, ParamValue};
use crate::ids::{AssetId, DeviceId, ParamId, TrackId};
use crate::time::{Db, Pan};

use super::{MetronomeState, PlanHandle, PlaybackSchedule, RampSpec};

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
