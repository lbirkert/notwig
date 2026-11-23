use serde::{Deserialize, Serialize};

use crate::domain::{AssetEntry, DeviceInstance, ProjectDoc};
use crate::ids::{ClipId, DeviceId, TrackId};
use crate::sequencer::ClipEntity;

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
