use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{DeviceChainId, SendId, TrackId};
use crate::time::{Color, Db, Pan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Instrument,
    Group,
    BusFx,
    Master,
}

impl TrackKind {
    #[must_use]
    pub fn has_lane(self) -> bool {
        matches!(self, Self::Audio | Self::Instrument)
    }

    #[must_use]
    pub fn can_have_children(self) -> bool {
        matches!(self, Self::Group)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Instrument => "instrument",
            Self::Group => "group",
            Self::BusFx => "bus-fx",
            Self::Master => "master",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerState {
    pub volume_db: Db,
    pub pan: Pan,
    pub mute: bool,
    pub solo: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            volume_db: Db(0.0),
            pan: Pan(0.0),
            mute: false,
            solo: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Send {
    pub id: SendId,
    pub target: TrackId,
    pub amount_db: Db,
    pub pre_fader: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackOutput {
    Default,
    Track(TrackId),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInputRef {
    pub driver: String,
    pub device: String,
    pub channel: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackIO {
    #[serde(default)]
    pub audio_input: Option<AudioInputRef>,
    pub midi_in: Option<String>,
    pub audio_out: TrackOutput,
}

impl Default for TrackIO {
    fn default() -> Self {
        Self {
            audio_input: None,
            midi_in: None,
            audio_out: TrackOutput::Default,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    /// True while the name is still the automatic name assigned by creation or
    /// an instrument insertion. Explicit renames clear it.
    #[serde(default)]
    pub name_is_auto: bool,
    pub color: Option<Color>,
    #[serde(default)]
    pub comment: String,
    #[serde(default = "default_true")]
    pub active: bool,
    pub parent: Option<TrackId>,
    pub mixer: MixerState,
    pub io: TrackIO,
    #[serde(default)]
    pub record_armed: bool,
    pub main_chain: DeviceChainId,
    pub sends: Vec<Send>,
}

impl Track {
    pub fn validate(&self, doc: &ProjectDoc) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::ValidationFailed {
                reason: "track name must not be empty".into(),
            });
        }
        if self.kind == TrackKind::Master {
            if self.parent.is_some() {
                return Err(CoreError::ValidationFailed {
                    reason: "Master must not have a parent".into(),
                });
            }
            if self.io.audio_out != TrackOutput::None {
                return Err(CoreError::ValidationFailed {
                    reason: "Master is the final sink and must not route onward".into(),
                });
            }
        }
        if let Some(parent) = self.parent {
            let Some(parent_track) = doc.tracks.get(&parent) else {
                return Err(CoreError::ValidationFailed {
                    reason: format!("track {} references missing parent", self.id),
                });
            };
            if !parent_track.kind.can_have_children() {
                return Err(CoreError::ValidationFailed {
                    reason: "only Group tracks may have children".into(),
                });
            }
        }
        if let TrackOutput::Track(target) = self.io.audio_out {
            if !doc.tracks.contains_key(&target) {
                return Err(CoreError::ValidationFailed {
                    reason: "track output references missing track".into(),
                });
            }
            if target == self.id {
                return Err(CoreError::ValidationFailed {
                    reason: "track cannot route to itself".into(),
                });
            }
        }
        let mut send_ids = BTreeSet::new();
        for send in &self.sends {
            if !send_ids.insert(send.id) {
                return Err(CoreError::DuplicateId {
                    id: send.id.to_string(),
                });
            }
            let Some(target) = doc.tracks.get(&send.target) else {
                return Err(CoreError::ValidationFailed {
                    reason: "send references missing target track".into(),
                });
            };
            if !matches!(
                target.kind,
                TrackKind::BusFx | TrackKind::Group | TrackKind::Master
            ) {
                return Err(CoreError::ValidationFailed {
                    reason: "send target must be BusFx, Group, or Master".into(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackForest {
    pub roots: Vec<TrackId>,
    pub children: BTreeMap<TrackId, Vec<TrackId>>,
}

impl TrackForest {
    pub fn insert(&mut self, id: TrackId, parent: Option<TrackId>, index: Option<usize>) {
        let siblings = match parent {
            None => &mut self.roots,
            Some(parent) => self.children.entry(parent).or_default(),
        };
        let index = index.unwrap_or(siblings.len()).min(siblings.len());
        siblings.insert(index, id);
    }

    pub fn remove(&mut self, id: TrackId) {
        self.detach(id);
        self.children.remove(&id);
    }

    fn detach(&mut self, id: TrackId) {
        if let Some(pos) = self.roots.iter().position(|root| *root == id) {
            self.roots.remove(pos);
        }
        for siblings in self.children.values_mut() {
            siblings.retain(|child| *child != id);
        }
    }

    pub fn move_node(
        &mut self,
        id: TrackId,
        parent: Option<TrackId>,
        index: usize,
    ) -> Result<(), CoreError> {
        if self.contains(id) {
            self.detach(id);
        }
        self.insert(id, parent, Some(index));
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, id: TrackId) -> bool {
        self.roots.contains(&id) || self.children.values().any(|v| v.contains(&id))
    }

    #[must_use]
    pub fn children_of(&self, id: TrackId) -> &[TrackId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn parent_of(&self, id: TrackId) -> Option<TrackId> {
        self.children
            .iter()
            .find_map(|(parent, children)| children.contains(&id).then_some(*parent))
    }

    pub fn depth_first(&self) -> Vec<TrackId> {
        let mut out = Vec::new();
        for root in &self.roots {
            self.visit(*root, &mut out);
        }
        out
    }

    fn visit(&self, id: TrackId, out: &mut Vec<TrackId>) {
        out.push(id);
        for child in self.children_of(id) {
            self.visit(*child, out);
        }
    }

    pub fn descendants(&self, id: TrackId) -> Vec<TrackId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            for child in self.children_of(current) {
                out.push(*child);
                stack.push(*child);
            }
        }
        out
    }

    pub fn validate(&self, tracks: &BTreeMap<TrackId, Track>) -> Result<(), CoreError> {
        let mut seen = BTreeSet::new();
        for root in &self.roots {
            if !seen.insert(*root) {
                return Err(CoreError::DuplicateId {
                    id: root.to_string(),
                });
            }
        }
        for (parent, children) in &self.children {
            if !tracks.contains_key(parent) {
                return Err(CoreError::ValidationFailed {
                    reason: format!("forest references missing parent {parent}"),
                });
            }
            for child in children {
                if !seen.insert(*child) {
                    return Err(CoreError::DuplicateId {
                        id: child.to_string(),
                    });
                }
                if !tracks.contains_key(child) {
                    return Err(CoreError::ValidationFailed {
                        reason: format!("forest references missing track {child}"),
                    });
                }
            }
        }
        if seen.len() != tracks.len() {
            return Err(CoreError::ValidationFailed {
                reason: "track forest does not contain every track exactly once".into(),
            });
        }
        for track in tracks.values() {
            let expected_parent = self.parent_of(track.id);
            if expected_parent != track.parent {
                return Err(CoreError::ValidationFailed {
                    reason: format!("track {} parent does not match forest", track.id),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    Audio(u16),
    Midi,
    NoteEvent,
    SidechainAudio(u16),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub from: TrackId,
    pub to: TrackId,
    pub signal: SignalType,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutingGraph {
    pub connections: Vec<Connection>,
}

impl RoutingGraph {
    pub fn validate(&self, doc: &ProjectDoc) -> Result<(), CoreError> {
        for connection in &self.connections {
            if !doc.tracks.contains_key(&connection.from)
                || !doc.tracks.contains_key(&connection.to)
            {
                return Err(CoreError::ValidationFailed {
                    reason: "routing connection references a missing track".into(),
                });
            }
        }
        Ok(())
    }

    pub fn routes_to(&self, id: TrackId) -> Vec<TrackId> {
        self.connections
            .iter()
            .filter(|c| c.from == id)
            .map(|c| c.to)
            .collect()
    }
}
