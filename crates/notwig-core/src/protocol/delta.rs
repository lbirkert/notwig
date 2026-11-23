use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::ProjectDoc;
use crate::ids::{AssetId, ClipId, DeviceChainId, DeviceId, NoteId, TrackId};

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
