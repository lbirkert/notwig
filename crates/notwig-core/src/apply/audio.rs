use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::ids::{ClipId, SliceId};
use crate::protocol::DocChange;
use crate::sequencer::{AudioClipData, AudioSlice, ClipEntity, ClipPayload, sort_lane};
use crate::time::{RangeMus, TimeMus};

use super::clips::trim_clip_start;
use super::dispatch::Changes;

/// Maps a slice into a requested musical window, returning `None` when the
/// window misses the slice or yields no audio.
///
/// The slice's extent is derived from its source sample count at the current
/// tempo, intersected with `from..to`, then converted back into source sample
/// offsets. Leading silence is carried as `silent_prefix_samples` rather than
/// counted as source material, so `source_start`/`source_end` stay inside the
/// original source window even when a window starts or ends within silence.
fn audio_slice_window(
    map: crate::time::TempoMap,
    slice: &AudioSlice,
    from: TimeMus,
    to: TimeMus,
    new_origin: TimeMus,
    new_id: bool,
) -> Option<AudioSlice> {
    let duration = map.samples_to_mus(slice.duration_samples());
    let start = slice.start;
    let end = start + duration;
    let window_start = from.max(start);
    let window_end = to.min(end);
    if window_end <= window_start {
        return None;
    }
    let offset_start = u64::try_from(map.samples_delta(window_start - start)).unwrap_or(0);
    let offset_end = u64::try_from(map.samples_delta(window_end - start)).unwrap_or(0);
    let silent = slice.silent_prefix_samples;
    // Skip the silent prefix before mapping into the source window.
    let audio_begin = offset_start.max(silent);
    let silent_prefix_samples = audio_begin - offset_start;
    let max_end = i128::from(slice.source_end_sample);
    let base = i128::from(slice.source_start_sample);
    let source_start = base + i128::from(audio_begin - silent);
    let source_end = if offset_end <= silent {
        source_start
    } else {
        base + i128::from(offset_end - silent)
    };
    let source_start = source_start.clamp(0, max_end);
    let source_end = source_end.clamp(source_start, max_end);
    // Silence-only windows still produce a segment; only an empty *source*
    // window with no remaining silence is dropped.
    if source_end <= source_start && silent_prefix_samples == 0 {
        return None;
    }
    let mut segment = slice.clone();
    if new_id {
        // Slices have no external identity, so cloning one into a new clip
        // mints a fresh ID here instead of taking one from the caller.
        segment.id = SliceId::from_uuid(crate::ids::new_uuid());
    }
    segment.start = window_start - new_origin;
    segment.source_start_sample = u64::try_from(source_start).unwrap_or(0);
    segment.source_end_sample = u64::try_from(source_end).unwrap_or(0);
    segment.silent_prefix_samples = silent_prefix_samples;
    Some(segment)
}

pub(crate) fn audio_slice_window_set(
    map: crate::time::TempoMap,
    audio: &AudioClipData,
    from: TimeMus,
    to: TimeMus,
    new_origin: TimeMus,
    new_ids: bool,
) -> Vec<AudioSlice> {
    audio
        .slices
        .iter()
        .filter_map(|slice| audio_slice_window(map, slice, from, to, new_origin, new_ids))
        .collect()
}

/// Keeps audio content anchored while the clip container changes.
///
/// Unsliced clips are sample-locked. Extending the container adds silence and
/// converts the clip to a single slice; shrinking it trims the source window.
/// Sliced clips keep their absolute positions and rely on engine-side clipping.
pub(super) fn resize_audio_clip_range(
    doc: &ProjectDoc,
    old: &ClipEntity,
    next: &mut ClipEntity,
) -> Result<(), CoreError> {
    let new_range = next.range;
    let ClipPayload::Audio(audio) = &mut next.payload else {
        return Ok(());
    };
    let map = doc.tempo_map();
    if audio.uses_slices() {
        let left_delta = new_range.start - old.range.start;
        for slice in &mut audio.slices {
            slice.start = slice.start - left_delta;
        }
        audio.slices_enabled = true;
        return Ok(());
    }
    let extends = new_range.start < old.range.start || new_range.end > old.range.end;
    let new_samples = map.mus_to_samples(new_range.length());
    if extends || new_samples > audio.source_duration() {
        let relative_start = old.range.start - new_range.start;
        audio.slices = vec![AudioSlice {
            // Converting the container to slices needs an internal slice ID;
            // slices are implementation details, not caller-owned objects.
            id: SliceId::from_uuid(crate::ids::new_uuid()),
            start: relative_start,
            source_start_sample: audio.source_start_sample,
            source_end_sample: audio.source_end_sample,
            silent_prefix_samples: 0,
            gain_db: audio.gain_db,
            asset_id: None,
        }];
        audio.slices_enabled = true;
        return Ok(());
    }
    let left = map.samples_delta(new_range.start - old.range.start);
    let right = map.samples_delta(old.range.end - new_range.end);
    let start = i128::from(audio.source_start_sample) + i128::from(left);
    let end = i128::from(audio.source_end_sample) - i128::from(right);
    if start < 0 || end <= start {
        return Err(CoreError::ValidationFailed {
            reason: "audio trim exceeds source material".into(),
        });
    }
    audio.source_start_sample = u64::try_from(start).unwrap_or(0);
    audio.source_end_sample = u64::try_from(end).unwrap_or(0);
    Ok(())
}

/// Unsliced audio stays sample-locked across tempo changes: the musical range
/// is rescaled so the same source sample count still fits the clip.
///
/// Rescaling only moves each clip's own end, so a faster tempo can make a clip
/// extend into its right-hand neighbour. Existing lane/clip helpers trim the
/// later clip's start back to the previous clip's end, preserving the
/// destination-clears-first rule. When that trim would consume the later clip
/// entirely or exceed its source material, the tempo change is rejected with a
/// typed error instead of leaving overlapping clips behind.
pub(super) fn rescale_unsliced_audio_clips(
    doc: &mut ProjectDoc,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let map = doc.tempo_map();
    let ids: Vec<ClipId> = doc.arranger.clips.keys().copied().collect();
    let mut lanes = std::collections::BTreeSet::new();
    for id in ids {
        let Some(clip) = doc.arranger.clips.get_mut(&id) else {
            continue;
        };
        let ClipPayload::Audio(audio) = &clip.payload else {
            continue;
        };
        if audio.uses_slices() {
            continue;
        }
        let length = map.samples_to_mus(audio.source_duration());
        let Ok(range) = RangeMus::new(clip.range.start, clip.range.start + length) else {
            continue;
        };
        if range == clip.range {
            continue;
        }
        clip.range = range;
        lanes.insert(clip.track_id);
        changes.affected.clips.insert(id);
        changes.push(DocChange::ClipUpserted(id));
    }
    for track_id in lanes {
        resolve_lane_overlaps(doc, track_id, changes)?;
    }
    Ok(())
}

fn resolve_lane_overlaps(
    doc: &mut ProjectDoc,
    track_id: crate::ids::TrackId,
    changes: &mut Changes,
) -> Result<(), CoreError> {
    let Some(mut lane) = doc.arranger.lanes.get(&track_id).cloned() else {
        return Ok(());
    };
    sort_lane(&mut lane, &doc.arranger.clips);
    let mut previous_end: Option<TimeMus> = None;
    let mut trimmed = false;
    for id in lane.clips {
        let Some(clip) = doc.arranger.clips.get(&id).cloned() else {
            continue;
        };
        if let Some(end) = previous_end
            && clip.range.start < end
        {
            if clip.range.end <= end {
                return Err(CoreError::Conflict {
                    reason: format!(
                        "tempo change makes clip {id} fully covered by its neighbour; refusing to drop content"
                    ),
                });
            }
            let new_start = end;
            if let ClipPayload::Audio(audio) = &clip.payload
                && !audio.uses_slices()
            {
                let consumed = doc.tempo_map().samples_delta(new_start - clip.range.start);
                if u64::try_from(consumed).unwrap_or(u64::MAX) >= audio.source_duration() {
                    return Err(CoreError::Conflict {
                        reason: format!(
                            "tempo change would overlap clip {id} and trimming it would exceed its source material"
                        ),
                    });
                }
            }
            trim_clip_start(doc, &clip, new_start)?;
            changes.affected.clips.insert(id);
            changes.push(DocChange::ClipUpserted(id));
            trimmed = true;
        }
        previous_end = Some(
            doc.arranger
                .clips
                .get(&id)
                .map_or(clip.range.end, |clip| clip.range.end),
        );
    }
    if trimmed && let Some(lane) = doc.arranger.lanes.get_mut(&track_id) {
        sort_lane(lane, &doc.arranger.clips);
    }
    Ok(())
}

pub(super) fn trim_audio_start(
    doc: &ProjectDoc,
    audio: &mut AudioClipData,
    delta: TimeMus,
    new_length: TimeMus,
) {
    if audio.uses_slices() {
        let map = doc.tempo_map();
        audio.slices = audio_slice_window_set(map, audio, delta, delta + new_length, delta, false);
        audio.slices_enabled = true;
    } else {
        let samples = doc.tempo_map().samples_delta(delta);
        audio.source_start_sample = audio
            .source_start_sample
            .saturating_add(u64::try_from(samples.max(0)).unwrap_or(0));
    }
}

pub(super) fn trim_audio_end(
    doc: &ProjectDoc,
    audio: &mut AudioClipData,
    delta: TimeMus,
    new_length: TimeMus,
) {
    if audio.uses_slices() {
        let map = doc.tempo_map();
        audio.slices = audio_slice_window_set(
            map,
            audio,
            TimeMus::zero(),
            new_length,
            TimeMus::zero(),
            false,
        );
        audio.slices_enabled = true;
    } else {
        let samples = doc.tempo_map().samples_delta(delta);
        audio.source_end_sample = audio
            .source_end_sample
            .saturating_sub(u64::try_from(samples.max(0)).unwrap_or(0));
    }
}
