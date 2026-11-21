use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{AssetId, ClipId, NoteId, TrackId};
use crate::time::{Color, Db, Norm01, RangeMus, TimeMus};
use crate::tracks::TrackKind;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArrangerState {
    pub lanes: BTreeMap<TrackId, Lane>,
    pub clips: BTreeMap<ClipId, ClipEntity>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub clips: Vec<ClipId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipKind {
    Midi,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipEntity {
    pub id: ClipId,
    pub track_id: TrackId,
    pub kind: ClipKind,
    pub range: RangeMus,
    pub name: Option<String>,
    pub color: Option<Color>,
    pub muted: bool,
    pub payload: ClipPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipPayload {
    Midi(MidiClipData),
    Audio(AudioClipData),
}

impl ClipPayload {
    #[must_use]
    pub fn kind(&self) -> ClipKind {
        match self {
            Self::Midi(_) => ClipKind::Midi,
            Self::Audio(_) => ClipKind::Audio,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiClipData {
    pub length: TimeMus,
    pub content_start: TimeMus,
    pub notes: Vec<NoteEntity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteEntity {
    pub id: NoteId,
    pub pitch: u8,
    pub start: TimeMus,
    pub length: TimeMus,
    pub velocity: Norm01,
    pub release_velocity: Option<Norm01>,
    pub expression: BTreeMap<String, ExprValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioClipData {
    pub asset_id: AssetId,
    pub source_start_sample: u64,
    pub source_end_sample: u64,
    pub gain_db: Db,
    pub fade_in_samples: u64,
    pub fade_out_samples: u64,
    pub fade_shape: FadeShape,
    #[serde(default)]
    pub slices: Vec<AudioSlice>,
    #[serde(default)]
    pub slices_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSlice {
    pub id: crate::ids::SliceId,
    pub start: TimeMus,
    pub source_start_sample: u64,
    pub source_end_sample: u64,
    #[serde(default)]
    pub silent_prefix_samples: u64,
    pub gain_db: Db,
    #[serde(default)]
    pub asset_id: Option<crate::ids::AssetId>,
}

impl AudioSlice {
    #[must_use]
    pub fn audio_samples(&self) -> u64 {
        self.source_end_sample
            .saturating_sub(self.source_start_sample)
    }

    #[must_use]
    pub fn duration_samples(&self) -> u64 {
        self.silent_prefix_samples
            .saturating_add(self.audio_samples())
    }

    #[must_use]
    pub fn source_sample_at(&self, offset: u64) -> Option<u64> {
        let audio_offset = offset.checked_sub(self.silent_prefix_samples)?;
        let sample = self.source_start_sample.checked_add(audio_offset)?;
        (sample < self.source_end_sample).then_some(sample)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeShape {
    Linear,
}

impl AudioClipData {
    #[must_use]
    pub fn source_duration(&self) -> u64 {
        self.source_end_sample
            .saturating_sub(self.source_start_sample)
    }

    #[must_use]
    pub fn uses_slices(&self) -> bool {
        self.slices_enabled || !self.slices.is_empty()
    }
}

impl ClipEntity {
    pub fn validate(&self, doc: &ProjectDoc) -> Result<(), CoreError> {
        if self.range.is_empty() {
            return Err(CoreError::ValidationFailed {
                reason: "clip range must be non-empty".into(),
            });
        }
        if self.range.start < TimeMus::zero() {
            return Err(CoreError::ValidationFailed {
                reason: "clip range must not start before zero".into(),
            });
        }
        let Some(track) = doc.tracks.get(&self.track_id) else {
            return Err(CoreError::ValidationFailed {
                reason: "clip references missing track".into(),
            });
        };
        if !track.kind.has_lane() {
            return Err(CoreError::ValidationFailed {
                reason: format!("{} tracks cannot host clips", track.kind.name()),
            });
        }
        if self.kind != self.payload.kind() {
            return Err(CoreError::ValidationFailed {
                reason: "clip kind does not match payload".into(),
            });
        }
        match (&track.kind, self.kind) {
            (TrackKind::Audio, ClipKind::Audio) | (TrackKind::Instrument, ClipKind::Midi) => {}
            _ => {
                return Err(CoreError::ValidationFailed {
                    reason: "clip kind is incompatible with track kind".into(),
                });
            }
        }

        match &self.payload {
            ClipPayload::Midi(midi) => midi.validate(),
            ClipPayload::Audio(audio) => audio.validate(self.range, doc),
        }
    }
}

impl MidiClipData {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.length < TimeMus::zero() {
            return Err(CoreError::ValidationFailed {
                reason: "MIDI content length must not be negative".into(),
            });
        }
        if self.content_start < TimeMus::zero() || self.content_start > self.length {
            return Err(CoreError::ValidationFailed {
                reason: "MIDI content start must lie within content length".into(),
            });
        }
        let mut ids = std::collections::BTreeSet::new();
        for note in &self.notes {
            if !ids.insert(note.id) {
                return Err(CoreError::DuplicateId {
                    id: note.id.to_string(),
                });
            }
            note.validate()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn note(&self, id: NoteId) -> Option<&NoteEntity> {
        self.notes.iter().find(|note| note.id == id)
    }

    pub fn note_mut(&mut self, id: NoteId) -> Option<&mut NoteEntity> {
        self.notes.iter_mut().find(|note| note.id == id)
    }
}

impl NoteEntity {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.pitch > 127 {
            return Err(CoreError::ValidationFailed {
                reason: "note pitch must be in 0..=127".into(),
            });
        }
        if self.length <= TimeMus::zero() {
            return Err(CoreError::ValidationFailed {
                reason: "note length must be positive".into(),
            });
        }
        if !(0.0..=1.0).contains(&self.velocity.0) {
            return Err(CoreError::ValidationFailed {
                reason: "note velocity must lie in [0,1]".into(),
            });
        }
        if let Some(release) = self.release_velocity {
            if !(0.0..=1.0).contains(&release.0) {
                return Err(CoreError::ValidationFailed {
                    reason: "note release velocity must lie in [0,1]".into(),
                });
            }
        }
        Ok(())
    }
}

impl AudioClipData {
    fn validate(&self, range: RangeMus, doc: &ProjectDoc) -> Result<(), CoreError> {
        if self.source_end_sample <= self.source_start_sample {
            return Err(CoreError::ValidationFailed {
                reason: "audio source range must be non-empty".into(),
            });
        }
        // An empty audio clip (slice mode with no events yet) may carry a nil
        // asset id. Sliced clips only need the top-level asset when at least one
        // slice falls back to it; slices with explicit asset ids are checked
        // individually below.
        let needs_top_level_asset = if self.uses_slices() {
            self.slices.iter().any(|slice| slice.asset_id.is_none())
        } else {
            true
        };
        if needs_top_level_asset && !doc.assets.assets.contains_key(&self.asset_id) {
            return Err(CoreError::ValidationFailed {
                reason: "audio clip references missing asset".into(),
            });
        }
        if self.uses_slices() {
            for slice in &self.slices {
                if slice.source_end_sample <= slice.source_start_sample
                    && slice.silent_prefix_samples == 0
                {
                    return Err(CoreError::ValidationFailed {
                        reason: "audio slice source range must be non-empty".into(),
                    });
                }
                if !doc
                    .assets
                    .assets
                    .contains_key(&slice.asset_id.unwrap_or(self.asset_id))
                {
                    return Err(CoreError::ValidationFailed {
                        reason: "audio slice references missing asset".into(),
                    });
                }
                // Audio events may extend beyond the clip container. The engine
                // clips them to the container for playback, and the audio editor
                // still shows the hidden part.
            }
        } else {
            let duration = self.source_duration();
            if self.fade_in_samples + self.fade_out_samples > duration {
                return Err(CoreError::ValidationFailed {
                    reason: "audio fades must not overlap".into(),
                });
            }
            let samples = doc.tempo_map().mus_to_samples(range.length());
            if samples != duration {
                return Err(CoreError::ValidationFailed {
                    reason: format!(
                        "audio clip range maps to {samples} samples but source duration is {duration}"
                    ),
                });
            }
        }
        Ok(())
    }
}

pub fn sort_lane(lane: &mut Lane, clips: &BTreeMap<ClipId, ClipEntity>) {
    lane.clips.sort_by(|a, b| {
        let a_start = clips
            .get(a)
            .map_or(TimeMus::zero(), |clip| clip.range.start);
        let b_start = clips
            .get(b)
            .map_or(TimeMus::zero(), |clip| clip.range.start);
        a_start.cmp(&b_start).then_with(|| a.cmp(b))
    });
}
