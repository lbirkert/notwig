use super::{
    EditError, EditModel, EditOp, EditorKind, LaneBounds, NextSelection, ObjectExtent, ObjectKind,
    ObjectRef, ObjectView, Plan, TrimLimits, min_clip_span,
};
use crate::apply::audio_slice_window_set;
use crate::domain::ProjectDoc;
use crate::ids::{ClipId, TrackId};
use crate::sequencer::{
    AudioClipData, AudioSlice, ClipEntity, ClipKind, ClipPayload, MidiClipData,
};
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

pub fn join_clips(
    model: &ArrangerModel<'_>,
    clip_ids: &[ClipId],
) -> Result<Plan<TrackId, ClipId, ClipContent>, EditError> {
    let mut ids = clip_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 2 {
        return Err(EditError::InvalidSelection(
            "select at least two clips to join".into(),
        ));
    }
    let mut clips: Vec<&ClipEntity> = ids
        .iter()
        .map(|id| {
            model
                .doc
                .arranger
                .clips
                .get(id)
                .ok_or_else(|| EditError::NotFound(id.to_string()))
        })
        .collect::<Result<_, _>>()?;
    clips.sort_by(|a, b| a.range.start.cmp(&b.range.start).then(a.id.cmp(&b.id)));
    let track_id = clips[0].track_id;
    let kind = clips[0].kind;
    if clips.iter().any(|clip| clip.track_id != track_id) {
        return Err(EditError::InvalidSelection(
            "clips must be on the same lane".into(),
        ));
    }
    if clips.iter().any(|clip| clip.kind != kind) {
        return Err(EditError::InvalidSelection(
            "clips must have the same kind".into(),
        ));
    }
    if !clips
        .windows(2)
        .all(|pair| pair[0].range.end == pair[1].range.start)
    {
        return Err(EditError::InvalidSelection("clips must be adjacent".into()));
    }
    let span = RangeMus::new(clips[0].range.start, clips[clips.len() - 1].range.end)
        .map_err(|err| EditError::InvalidSelection(err.to_string()))?;
    let payload = match kind {
        ClipKind::Midi => {
            let content_start = clips
                .iter()
                .filter_map(|clip| match &clip.payload {
                    ClipPayload::Midi(midi) => {
                        Some(midi.content_start - (clip.range.start - span.start))
                    }
                    ClipPayload::Audio(_) => None,
                })
                .max()
                .unwrap_or(TimeMus::zero())
                .max(TimeMus::zero());
            let mut notes = Vec::new();
            for clip in &clips {
                let ClipPayload::Midi(midi) = &clip.payload else {
                    continue;
                };
                let offset = clip.range.start - span.start + content_start - midi.content_start;
                for note in &midi.notes {
                    let mut note = note.clone();
                    note.start = note.start + offset;
                    notes.push(note);
                }
            }
            notes.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
            ClipPayload::Midi(MidiClipData {
                length: content_start + span.length(),
                content_start,
                notes,
            })
        }
        ClipKind::Audio => {
            let mut slices = Vec::new();
            for clip in &clips {
                let ClipPayload::Audio(audio) = &clip.payload else {
                    continue;
                };
                if audio.uses_slices() {
                    for slice in &audio.slices {
                        let mut slice = slice.clone();
                        slice.start = slice.start + (clip.range.start - span.start);
                        slices.push(slice);
                    }
                } else {
                    slices.push(AudioSlice {
                        id: crate::ids::SliceId::nil(),
                        start: clip.range.start - span.start,
                        source_start_sample: audio.source_start_sample,
                        source_end_sample: audio.source_end_sample,
                        silent_prefix_samples: 0,
                        gain_db: audio.gain_db,
                        asset_id: Some(audio.asset_id),
                    });
                }
            }
            let first = match &clips[0].payload {
                ClipPayload::Audio(audio) => audio,
                ClipPayload::Midi(_) => unreachable!(),
            };
            ClipPayload::Audio(AudioClipData {
                asset_id: first.asset_id,
                source_start_sample: first.source_start_sample,
                source_end_sample: first.source_end_sample,
                gain_db: first.gain_db,
                gain_curve: first.gain_curve.clone(),
                fade_in_samples: first.fade_in_samples,
                fade_out_samples: first.fade_out_samples,
                fade_shape: first.fade_shape,
                slices,
                slices_enabled: true,
            })
        }
    };
    let mut ops = Vec::with_capacity(clips.len() + 1);
    for clip in &clips {
        ops.push(EditOp::Remove { object: clip.id });
    }
    let place_index = ops.len();
    ops.push(EditOp::Place {
        lane: track_id,
        span,
        kind: match kind {
            ClipKind::Midi => ObjectKind::ClipMidi,
            ClipKind::Audio => ObjectKind::ClipAudio,
        },
        content: ClipContent {
            kind,
            payload,
            name: clips[0].name.clone(),
            color: clips[0].color,
            muted: clips.iter().all(|clip| clip.muted),
        },
    });
    Ok(Plan {
        ops,
        next: NextSelection::Objects(vec![ObjectRef::Created(place_index)]),
    })
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
            ClipPayload::Midi(_) => (TimeMus::zero(), None),
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
                    gain_curve: shifted_curve(audio.gain_curve.as_ref(), left),
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
                    gain_curve: shifted_curve(audio.gain_curve.as_ref(), left),
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

fn shifted_curve(curve: Option<&crate::Curve>, offset: TimeMus) -> Option<crate::Curve> {
    let curve = curve?;
    crate::Curve::from_points(curve.points().iter().copied().map(|mut point| {
        point.time = point.time - offset;
        point
    }))
    .ok()
}
