use super::{
    EditError, EditModel, EditorKind, LaneBounds, ObjectExtent, ObjectKind, ObjectView, TrimLimits,
    min_clip_span,
};
use crate::apply::audio_slice_window_set;
use crate::domain::ProjectDoc;
use crate::ids::{ClipId, TrackId};
use crate::sequencer::{AudioClipData, ClipEntity, ClipKind, ClipPayload, MidiClipData};
use crate::time::{Color, RangeMus, TimeMus};
use crate::tracks::TrackKind;

#[derive(Debug, Clone, PartialEq)]
pub struct ClipContent {
    pub kind: ClipKind,
    pub payload: ClipPayload,
    pub name: Option<String>,
    pub color: Option<Color>,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ArrangerModel<'a> {
    doc: &'a ProjectDoc,
}

impl<'a> ArrangerModel<'a> {
    #[must_use]
    pub fn new(doc: &'a ProjectDoc) -> Self {
        Self { doc }
    }

    fn view(clip: &ClipEntity) -> ObjectView<TrackId, ClipId> {
        ObjectView {
            id: clip.id,
            lane: clip.track_id,
            span: clip.range,
            kind: match clip.kind {
                ClipKind::Midi => ObjectKind::ClipMidi,
                ClipKind::Audio => ObjectKind::ClipAudio,
            },
            extent: ObjectExtent::Span,
        }
    }

    fn trim_limits_for(self, clip: &ClipEntity) -> TrimLimits {
        let min_span = min_clip_span();
        let max_start = clip.range.end - min_span;
        let min_end = clip.range.start + min_span;
        let (min_start, max_end) = match &clip.payload {
            ClipPayload::Midi(midi) => (
                (clip.range.start - midi.content_start).max(TimeMus::zero()),
                None,
            ),
            ClipPayload::Audio(audio) if audio.uses_slices() => (TimeMus::zero(), None),
            ClipPayload::Audio(audio) => {
                let map = self.doc.tempo_map();
                let left = map.samples_to_mus(audio.source_start_sample);
                let min_start = (clip.range.start - left).max(TimeMus::zero());
                let asset_end = self
                    .doc
                    .assets
                    .assets
                    .get(&audio.asset_id)
                    .and_then(|entry| entry.duration_samples)
                    .unwrap_or(audio.source_end_sample);
                let remaining = asset_end.saturating_sub(audio.source_end_sample);
                let max_end = clip.range.end + map.samples_to_mus(remaining);
                (min_start, Some(max_end))
            }
        };
        TrimLimits {
            min_start,
            max_start,
            min_end,
            max_end,
        }
    }

    fn extract_content(
        self,
        clip: &ClipEntity,
        window: RangeMus,
    ) -> Result<ClipContent, EditError> {
        if window.is_empty() || window.start < clip.range.start || window.end > clip.range.end {
            return Err(EditError::InvalidExtraction(
                "window lies outside the clip".into(),
            ));
        }
        let left = window.start - clip.range.start;
        let length = window.length();
        let payload = match &clip.payload {
            ClipPayload::Midi(midi) => {
                let mut notes = Vec::new();
                for note in &midi.notes {
                    let absolute_start = clip.range.start + (note.start - midi.content_start);
                    let absolute_end = absolute_start + note.length;
                    let start = absolute_start.max(window.start);
                    let end = absolute_end.min(window.end);
                    if end <= start {
                        continue;
                    }
                    let mut clipped = note.clone();
                    clipped.start = start - window.start;
                    clipped.length = end - start;
                    notes.push(clipped);
                }
                ClipPayload::Midi(MidiClipData {
                    length,
                    content_start: TimeMus::zero(),
                    notes,
                })
            }
            ClipPayload::Audio(audio) if audio.uses_slices() => {
                let map = self.doc.tempo_map();
                let slices = audio_slice_window_set(map, audio, left, left + length, left, false);
                ClipPayload::Audio(AudioClipData {
                    asset_id: audio.asset_id,
                    source_start_sample: audio.source_start_sample,
                    source_end_sample: audio.source_end_sample,
                    gain_db: audio.gain_db,
                    fade_in_samples: 0,
                    fade_out_samples: 0,
                    fade_shape: audio.fade_shape,
                    slices,
                    slices_enabled: true,
                })
            }
            ClipPayload::Audio(audio) => {
                let map = self.doc.tempo_map();
                let start_delta = map.samples_delta(left);
                let source_start_i =
                    i128::from(audio.source_start_sample) + i128::from(start_delta);
                let source_start = u64::try_from(source_start_i)
                    .map_err(|_| EditError::InvalidExtraction("negative source start".into()))?;
                let source_end = source_start.saturating_add(map.mus_to_samples(length));
                if source_end <= source_start {
                    return Err(EditError::InvalidExtraction(
                        "empty audio source window".into(),
                    ));
                }
                ClipPayload::Audio(AudioClipData {
                    asset_id: audio.asset_id,
                    source_start_sample: source_start,
                    source_end_sample: source_end,
                    gain_db: audio.gain_db,
                    fade_in_samples: 0,
                    fade_out_samples: 0,
                    fade_shape: audio.fade_shape,
                    slices: Vec::new(),
                    slices_enabled: false,
                })
            }
        };
        Ok(ClipContent {
            kind: clip.kind,
            payload,
            name: clip.name.clone(),
            color: clip.color,
            muted: clip.muted,
        })
    }
}

impl<'a> ArrangerModel<'a> {
    #[must_use]
    pub fn clip(&self, id: ClipId) -> Option<&'a ClipEntity> {
        self.doc.arranger.clips.get(&id)
    }
}

impl EditModel for ArrangerModel<'_> {
    type Lane = TrackId;
    type Object = ClipId;
    type Content = ClipContent;

    fn editor(&self) -> EditorKind {
        EditorKind::Arranger
    }

    fn lanes(&self) -> Vec<TrackId> {
        self.doc
            .track_order
            .depth_first()
            .into_iter()
            .filter(|id| {
                self.doc
                    .tracks
                    .get(id)
                    .is_some_and(|track| track.kind.has_lane())
            })
            .collect()
    }

    fn lane_bounds(&self, _lane: TrackId) -> LaneBounds {
        LaneBounds {
            start: TimeMus::zero(),
            end: None,
        }
    }

    fn objects(&self, lane: TrackId) -> Vec<ObjectView<TrackId, ClipId>> {
        let Some(lane_data) = self.doc.arranger.lanes.get(&lane) else {
            return Vec::new();
        };
        let mut views: Vec<_> = lane_data
            .clips
            .iter()
            .filter_map(|id| self.doc.arranger.clips.get(id))
            .map(Self::view)
            .collect();
        views.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.id.cmp(&b.id)));
        views
    }

    fn object(&self, object: ClipId) -> Option<ObjectView<TrackId, ClipId>> {
        self.doc.arranger.clips.get(&object).map(Self::view)
    }

    fn trim_limits(&self, object: ClipId) -> TrimLimits {
        match self.doc.arranger.clips.get(&object) {
            Some(clip) => self.trim_limits_for(clip),
            None => TrimLimits {
                min_start: TimeMus::zero(),
                max_start: TimeMus::zero(),
                min_end: TimeMus::zero(),
                max_end: Some(TimeMus::zero()),
            },
        }
    }

    fn extract(&self, object: ClipId, window: RangeMus) -> Result<ClipContent, EditError> {
        let clip = self
            .doc
            .arranger
            .clips
            .get(&object)
            .ok_or_else(|| EditError::NotFound(object.to_string()))?;
        self.extract_content(clip, window)
    }

    fn accepts(&self, lane: TrackId, kind: ObjectKind) -> bool {
        let Some(track) = self.doc.tracks.get(&lane) else {
            return false;
        };
        matches!(
            (track.kind, kind),
            (TrackKind::Instrument, ObjectKind::ClipMidi)
                | (TrackKind::Audio, ObjectKind::ClipAudio)
        )
    }
}
