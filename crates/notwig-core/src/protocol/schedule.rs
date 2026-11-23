use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::ProjectDoc;
use crate::ids::TrackId;
use crate::sequencer::{ClipEntity, ClipPayload};
use crate::time::TimeMus;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSchedule {
    pub tracks: BTreeMap<TrackId, Vec<ClipEntity>>,
}

impl PlaybackSchedule {
    #[must_use]
    pub fn from_doc(doc: &ProjectDoc) -> Self {
        let mut tracks: BTreeMap<TrackId, Vec<ClipEntity>> = BTreeMap::new();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetronomeState {
    pub enabled: bool,
    pub beats_per_bar: u8,
    /// Length of one click pulse in quarter-note beats.
    pub beat_length: TimeMus,
    /// Length of one bar in quarter-note beats.
    pub bar_length: TimeMus,
}
