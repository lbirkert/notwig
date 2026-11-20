use notwig_core::edit::{
    EditError, EditModel, EditOp, EditorKind, LaneBounds, NextSelection, ObjectExtent, ObjectKind,
    ObjectView, Plan, TrimLimits, epsilon,
};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{RangeMus, SliceId, TempoMap, TimeMus};
use uuid::Uuid;

use crate::intent::UiIntent;
use crate::shell::NotwigUi;

use super::geometry::samples_for_delta;
use super::state::ClipEditorState;

pub(super) fn editor_slices(
    clip: &notwig_core::ClipEntity,
    audio: &AudioClipData,
) -> Vec<AudioSlice> {
    crate::timeline::audio_slices(clip, audio)
}

pub fn source_duration(map: &TempoMap, slice: &AudioSlice) -> TimeMus {
    map.samples_to_mus(slice.duration_samples())
}

pub(super) fn adjust_trim_start(
    silent_prefix: u64,
    source_start: u64,
    source_end: u64,
    delta_samples: i64,
) -> (u64, u64) {
    if delta_samples < 0 {
        let reveal = delta_samples.unsigned_abs();
        let from_source = reveal.min(source_start);
        (
            silent_prefix.saturating_add(reveal - from_source),
            source_start - from_source,
        )
    } else {
        let shrink = delta_samples.unsigned_abs();
        let from_silent = shrink.min(silent_prefix);
        let remaining = shrink - from_silent;
        let audio = source_end.saturating_sub(source_start);
        let advance = remaining.min(audio.saturating_sub(1));
        (silent_prefix - from_silent, source_start + advance)
    }
}

fn slice_interval(map: &TempoMap, slice: &AudioSlice) -> Option<RangeMus> {
    RangeMus::new(slice.start, slice.start + source_duration(map, slice)).ok()
}

pub(super) fn slice_intersects(map: &TempoMap, slice: &AudioSlice, range: RangeMus) -> bool {
    slice_interval(map, slice).is_some_and(|interval| interval.intersects(range))
}

fn slice_segment(
    map: &TempoMap,
    slice: &AudioSlice,
    from: TimeMus,
    to: TimeMus,
) -> Option<AudioSlice> {
    let interval = slice_interval(map, slice)?;
    let start = from.max(interval.start);
    let end = to.min(interval.end);
    if end <= start {
        return None;
    }
    let offset_start = u64::try_from(samples_for_delta(start - interval.start, map)).unwrap_or(0);
    let offset_end = u64::try_from(samples_for_delta(end - interval.start, map)).unwrap_or(0);
    let silent = slice.silent_prefix_samples;
    let audio_begin = offset_start.max(silent);
    let silent_prefix_samples = audio_begin - offset_start;
    let base = i128::from(slice.source_start_sample);
    let max_end = i128::from(slice.source_end_sample);
    let source_start = base + i128::from(audio_begin - silent);
    let source_end = if offset_end <= silent {
        source_start
    } else {
        base + i128::from(offset_end - silent)
    };
    let source_start = source_start.clamp(0, max_end);
    let source_end = source_end.clamp(source_start, max_end);
    if source_end <= source_start && silent_prefix_samples == 0 {
        return None;
    }
    let mut segment = slice.clone();
    segment.start = start;
    segment.source_start_sample = u64::try_from(source_start).unwrap_or(0);
    segment.source_end_sample = u64::try_from(source_end).unwrap_or(0);
    segment.silent_prefix_samples = silent_prefix_samples;
    Some(segment)
}

pub fn delete_range(map: &TempoMap, slices: &[AudioSlice], range: RangeMus) -> Vec<AudioSlice> {
    if range.is_empty() {
        return slices.to_vec();
    }
    let mut out = Vec::new();
    for slice in slices {
        let Some(interval) = slice_interval(map, slice) else {
            continue;
        };
        if !interval.intersects(range) {
            out.push(slice.clone());
            continue;
        }
        let mut parts = Vec::new();
        if interval.start < range.start
            && let Some(left) = slice_segment(map, slice, interval.start, range.start)
        {
            parts.push(left);
        }
        if interval.end > range.end
            && let Some(right) = slice_segment(map, slice, range.end, interval.end)
        {
            parts.push(right);
        }
        for (index, mut part) in parts.into_iter().enumerate() {
            if index > 0 {
                part.id = SliceId::from_uuid(Uuid::new_v4());
            }
            out.push(part);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipLane;

pub struct SliceModel<'a> {
    pub(super) slices: &'a [AudioSlice],
    pub(super) map: TempoMap,
    pub(super) clip_length: TimeMus,
    pub(super) clip_start: TimeMus,
}

impl SliceModel<'_> {
    fn span(&self, slice: &AudioSlice) -> RangeMus {
        RangeMus::new(slice.start, slice.start + source_duration(&self.map, slice))
            .unwrap_or_else(|_| RangeMus::new(slice.start, slice.start).unwrap())
    }
}

impl EditModel for SliceModel<'_> {
    type Lane = ClipLane;
    type Object = SliceId;
    type Content = AudioSlice;

    fn editor(&self) -> EditorKind {
        EditorKind::AudioClip
    }

    fn container(&self) -> Option<RangeMus> {
        RangeMus::new(TimeMus::zero(), self.clip_length).ok()
    }

    fn lanes(&self) -> Vec<ClipLane> {
        vec![ClipLane]
    }

    fn lane_bounds(&self, _lane: ClipLane) -> LaneBounds {
        LaneBounds {
            start: TimeMus::zero() - self.clip_start,
            end: None,
        }
    }

    fn objects(&self, _lane: ClipLane) -> Vec<ObjectView<ClipLane, SliceId>> {
        let mut out: Vec<_> = self
            .slices
            .iter()
            .map(|slice| ObjectView {
                id: slice.id,
                lane: ClipLane,
                span: self.span(slice),
                kind: ObjectKind::Slice,
                extent: ObjectExtent::Span,
            })
            .collect();
        out.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.id.cmp(&b.id)));
        out
    }

    fn object(&self, object: SliceId) -> Option<ObjectView<ClipLane, SliceId>> {
        self.slices
            .iter()
            .find(|slice| slice.id == object)
            .map(|slice| ObjectView {
                id: slice.id,
                lane: ClipLane,
                span: self.span(slice),
                kind: ObjectKind::Slice,
                extent: ObjectExtent::Span,
            })
    }

    fn trim_limits(&self, object: SliceId) -> TrimLimits {
        let Some(slice) = self.slices.iter().find(|slice| slice.id == object) else {
            return TrimLimits {
                min_start: TimeMus::zero(),
                max_start: TimeMus::zero(),
                min_end: TimeMus::zero(),
                max_end: Some(TimeMus::zero()),
            };
        };
        let span = self.span(slice);
        TrimLimits {
            min_start: TimeMus::zero() - self.clip_start,
            max_start: span.end - epsilon(),
            min_end: span.start + epsilon(),
            max_end: None,
        }
    }

    fn extract(&self, object: SliceId, window: RangeMus) -> Result<AudioSlice, EditError> {
        let slice = self
            .slices
            .iter()
            .find(|slice| slice.id == object)
            .ok_or_else(|| EditError::NotFound(object.to_string()))?;
        let span = self.span(slice);
        if window.is_empty() || window.start < span.start || window.end > span.end {
            return Err(EditError::InvalidExtraction(
                "window lies outside the slice".into(),
            ));
        }
        let mut segment = slice.clone();
        let offset_start =
            u64::try_from(self.map.samples_delta(window.start - slice.start)).unwrap_or(0);
        let offset_end =
            u64::try_from(self.map.samples_delta(window.end - slice.start)).unwrap_or(0);
        let silent = slice.silent_prefix_samples;
        let audio_begin = offset_start.max(silent);
        let silent_prefix_samples = audio_begin - offset_start;
        let base = i128::from(slice.source_start_sample);
        let max_end = i128::from(slice.source_end_sample);
        let source_start = base + i128::from(audio_begin - silent);
        let source_end = if offset_end <= silent {
            source_start
        } else {
            base + i128::from(offset_end - silent)
        };
        let source_start = source_start.clamp(0, max_end);
        let source_end = source_end.clamp(source_start, max_end);
        if source_end <= source_start && silent_prefix_samples == 0 {
            return Err(EditError::InvalidExtraction("empty audio window".into()));
        }
        segment.start = TimeMus::zero();
        segment.source_start_sample = u64::try_from(source_start).unwrap_or(0);
        segment.source_end_sample = u64::try_from(source_end).unwrap_or(0);
        segment.silent_prefix_samples = silent_prefix_samples;
        Ok(segment)
    }

    fn accepts(&self, _lane: ClipLane, _kind: ObjectKind) -> bool {
        true
    }
}

pub enum SliceSelection {
    Unchanged,
    None,
    Objects(Vec<(SliceId, RangeMus)>),
    Time(RangeMus),
}

pub struct SliceLowered {
    pub slices: Vec<AudioSlice>,
    pub selection: SliceSelection,
}

pub fn lower_slice_plan(
    slices: &[AudioSlice],
    map: &TempoMap,
    plan: Plan<ClipLane, SliceId, AudioSlice>,
) -> SliceLowered {
    let mut out = slices.to_vec();
    let mut placed: Vec<Option<(SliceId, RangeMus)>> = vec![None; plan.ops.len()];
    for (index, op) in plan.ops.iter().enumerate() {
        match op {
            EditOp::SetContainer { .. } => {}
            EditOp::Erase { span, .. } => {
                out = delete_range(map, &out, *span);
            }
            EditOp::Remove { object } => {
                out.retain(|slice| slice.id != *object);
            }
            EditOp::Move { object, span, .. } => {
                if let Some(slice) = out.iter_mut().find(|slice| slice.id == *object) {
                    slice.start = span.start;
                }
            }
            EditOp::Trim { object, span } => {
                trim_slice(map, &mut out, *object, *span);
            }
            EditOp::Place { span, content, .. } => {
                let mut copy = content.clone();
                copy.id = SliceId::from_uuid(Uuid::new_v4());
                copy.start = span.start;
                placed[index] = Some((copy.id, *span));
                out.push(copy);
            }
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    let selection = match plan.next {
        NextSelection::Unchanged => SliceSelection::Unchanged,
        NextSelection::None => SliceSelection::None,
        NextSelection::Time(time) => SliceSelection::Time(time.span),
        NextSelection::Objects(refs) => {
            let mut objects = Vec::new();
            for reference in refs {
                match reference {
                    notwig_core::edit::ObjectRef::Existing(id) => {
                        if let Some(slice) = out.iter().find(|slice| slice.id == id) {
                            let span = RangeMus::new(
                                slice.start,
                                slice.start + source_duration(map, slice),
                            )
                            .unwrap_or_else(|_| RangeMus::new(slice.start, slice.start).unwrap());
                            objects.push((id, span));
                        }
                    }
                    notwig_core::edit::ObjectRef::Created(index) => {
                        if let Some((id, span)) = placed.get(index).and_then(|entry| *entry) {
                            objects.push((id, span));
                        }
                    }
                }
            }
            SliceSelection::Objects(objects)
        }
    };
    SliceLowered {
        slices: out,
        selection,
    }
}

fn trim_slice(map: &TempoMap, slices: &mut [AudioSlice], id: SliceId, span: RangeMus) {
    let Some(slice) = slices.iter_mut().find(|slice| slice.id == id) else {
        return;
    };
    let old_start = slice.start;
    let old_end = old_start + source_duration(map, slice);
    if span.start != old_start && span.end == old_end {
        let delta = map.samples_delta(span.start - old_start);
        let (silent, source_start) = adjust_trim_start(
            slice.silent_prefix_samples,
            slice.source_start_sample,
            slice.source_end_sample,
            delta,
        );
        slice.silent_prefix_samples = silent;
        slice.source_start_sample = source_start;
        slice.start = span.start;
    } else if span.end != old_end && span.start == old_start {
        let delta = map.samples_delta(span.end - old_end);
        let min = i128::from(slice.source_start_sample.saturating_add(1));
        let source = (i128::from(slice.source_end_sample) + i128::from(delta)).max(min);
        slice.source_end_sample =
            u64::try_from(source).unwrap_or(slice.source_start_sample.saturating_add(1));
    }
}

pub fn emit_slices(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    slices: Vec<AudioSlice>,
) {
    audio.slices.clone_from(&slices);
    audio.slices_enabled = true;
    state
        .selected
        .retain(|id| slices.iter().any(|slice| slice.id == *id));
    ui.emit(UiIntent::SetClipSlices {
        clip_id: state.clip_id,
        slices,
    });
}
