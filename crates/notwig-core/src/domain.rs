use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::ids::{AssetId, DeviceChainId, DeviceId, ParamId};
use crate::sequencer::ArrangerState;
use crate::time::{Color, RangeMus, SampleRate, SchemaVersion, TimeMus};
use crate::tracks::{RoutingGraph, Track, TrackForest};

pub type OrderedMap<K, V> = IndexMap<K, V>;

pub const SCHEMA_VERSION: SchemaVersion = SchemaVersion::V0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    pub created_utc: i64,
    pub modified_utc: i64,
    pub sample_rate: SampleRate,
    pub tempo_bpm: f64,
    pub time_sig_num: u8,
    pub time_sig_den: u8,
}

impl Meta {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::ValidationFailed {
                reason: "project name must not be empty".into(),
            });
        }
        if !(1.0..=960.0).contains(&self.tempo_bpm) || !self.tempo_bpm.is_finite() {
            return Err(CoreError::ValidationFailed {
                reason: "tempo must be finite and between 1 and 960 bpm".into(),
            });
        }
        if self.time_sig_num == 0 || self.time_sig_den == 0 {
            return Err(CoreError::ValidationFailed {
                reason: "time signature numerator and denominator must be positive".into(),
            });
        }
        if !self.time_sig_den.is_power_of_two() {
            return Err(CoreError::ValidationFailed {
                reason: "time signature denominator must be a power of two".into(),
            });
        }
        SampleRate::new(self.sample_rate.value())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportDoc {
    pub playing: bool,
    pub recording: bool,
    pub playhead: TimeMus,
    pub loop_enabled: bool,
    pub loop_range: Option<RangeMus>,
}

impl Default for TransportDoc {
    fn default() -> Self {
        Self {
            playing: false,
            recording: false,
            playhead: TimeMus::zero(),
            loop_enabled: false,
            loop_range: None,
        }
    }
}

impl TransportDoc {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.playhead < TimeMus::zero() {
            return Err(CoreError::ValidationFailed {
                reason: "playhead must not be negative".into(),
            });
        }
        if self.loop_enabled {
            match self.loop_range {
                Some(range) if !range.is_empty() => {}
                _ => {
                    return Err(CoreError::ValidationFailed {
                        reason: "enabled loop requires a non-empty loop range".into(),
                    });
                }
            }
        }
        if let Some(range) = self.loop_range {
            if range.start < TimeMus::zero() {
                return Err(CoreError::ValidationFailed {
                    reason: "loop range must not start before zero".into(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetEntry {
    pub id: AssetId,
    pub kind: AssetKind,
    pub uri: String,
    pub hash: Option<String>,
    pub format: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration_samples: Option<u64>,
    pub original_sample_rate: Option<u32>,
    pub original_channels: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Audio,
    Midi,
    Preset,
    PluginState,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetIndex {
    pub assets: BTreeMap<AssetId, AssetEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceKind {
    Instrument,
    AudioFx,
    NoteFx,
    Container,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    EnumIndex(u32),
}

impl ParamValue {
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(v) => Some(*v),
            Self::I64(v) => Some(*v as f64),
            Self::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Self::EnumIndex(v) => Some(f64::from(*v)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateBlobRef {
    pub hash: String,
    pub encoding: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInstance {
    pub id: DeviceId,
    pub type_id: String,
    pub version: SchemaVersion,
    pub kind: DeviceKind,
    pub name: Option<String>,
    pub param_values: OrderedMap<ParamId, ParamValue>,
    pub state_ref: Option<StateBlobRef>,
    pub bypass: bool,
    pub child_chains: Vec<DeviceChainId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceChain {
    pub id: DeviceChainId,
    pub devices: Vec<DeviceId>,
}

impl DeviceChain {
    #[must_use]
    pub fn new(id: DeviceChainId) -> Self {
        Self {
            id,
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDoc {
    pub schema: SchemaVersion,
    pub revision: u64,
    pub meta: Meta,
    pub transport: TransportDoc,
    pub tracks: BTreeMap<crate::ids::TrackId, Track>,
    pub track_order: TrackForest,
    pub arranger: ArrangerState,
    pub devices: BTreeMap<DeviceId, DeviceInstance>,
    pub chains: BTreeMap<DeviceChainId, DeviceChain>,
    pub assets: AssetIndex,
    pub routing: RoutingGraph,
}

impl ProjectDoc {
    pub fn new_template(
        meta: Meta,
        master_id: crate::ids::TrackId,
        master_chain_id: DeviceChainId,
    ) -> Result<Self, CoreError> {
        meta.validate()?;
        let mut doc = Self {
            schema: SCHEMA_VERSION,
            revision: 0,
            meta,
            transport: TransportDoc::default(),
            tracks: BTreeMap::new(),
            track_order: TrackForest::default(),
            arranger: ArrangerState::default(),
            devices: BTreeMap::new(),
            chains: BTreeMap::new(),
            assets: AssetIndex::default(),
            routing: RoutingGraph::default(),
        };
        crate::apply::create_track(
            &mut doc,
            master_id,
            master_chain_id,
            crate::tracks::TrackKind::Master,
            "Master".into(),
            None,
            None,
            None,
        )?;
        Ok(doc)
    }

    #[must_use]
    pub fn tempo_map(&self) -> crate::time::TempoMap {
        crate::time::TempoMap {
            tempo_bpm: self.meta.tempo_bpm,
            sample_rate: self.meta.sample_rate,
        }
    }

    pub fn master_track(&self) -> Option<crate::ids::TrackId> {
        self.tracks
            .values()
            .find(|track| track.kind == crate::tracks::TrackKind::Master)
            .map(|track| track.id)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        self.meta.validate()?;
        self.transport.validate()?;
        self.track_order.validate(&self.tracks)?;

        let masters = self
            .tracks
            .values()
            .filter(|t| t.kind == crate::tracks::TrackKind::Master)
            .count();
        if masters != 1 {
            return Err(CoreError::ValidationFailed {
                reason: "document must contain exactly one Master track".into(),
            });
        }

        for track in self.tracks.values() {
            if !self.chains.contains_key(&track.main_chain) {
                return Err(CoreError::ValidationFailed {
                    reason: format!("track {} references a missing main chain", track.id),
                });
            }
            track.validate(self)?;
        }

        for (id, chain) in &self.chains {
            if id != &chain.id {
                return Err(CoreError::ValidationFailed {
                    reason: "device chain key/id mismatch".into(),
                });
            }
            let mut seen = std::collections::BTreeSet::new();
            for device_id in &chain.devices {
                if !seen.insert(*device_id) {
                    return Err(CoreError::DuplicateId {
                        id: device_id.to_string(),
                    });
                }
                if !self.devices.contains_key(device_id) {
                    return Err(CoreError::ValidationFailed {
                        reason: format!("chain {} references missing device {device_id}", chain.id),
                    });
                }
            }
        }

        for device in self.devices.values() {
            for child in &device.child_chains {
                if !self.chains.contains_key(child) {
                    return Err(CoreError::ValidationFailed {
                        reason: format!("device {} references missing child chain", device.id),
                    });
                }
            }
        }

        for (track_id, lane) in &self.arranger.lanes {
            if !self.tracks.contains_key(track_id) {
                return Err(CoreError::ValidationFailed {
                    reason: format!("lane references missing track {track_id}"),
                });
            }
            for clip_id in &lane.clips {
                let Some(clip) = self.arranger.clips.get(clip_id) else {
                    return Err(CoreError::ValidationFailed {
                        reason: format!("lane references missing clip {clip_id}"),
                    });
                };
                if clip.track_id != *track_id {
                    return Err(CoreError::ValidationFailed {
                        reason: "clip track_id does not match owning lane".into(),
                    });
                }
                clip.validate(self)?;
            }
        }

        for clip in self.arranger.clips.values() {
            if !self
                .arranger
                .lanes
                .get(&clip.track_id)
                .is_some_and(|lane| lane.clips.contains(&clip.id))
            {
                return Err(CoreError::ValidationFailed {
                    reason: format!("clip {} is not present in its lane", clip.id),
                });
            }
        }

        self.routing.validate(self)?;
        Ok(())
    }

    pub fn allocate_revision(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAssetBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioAssetBuffer {
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() as u64 / u64::from(self.channels)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ColorPalette(pub Vec<Color>);
