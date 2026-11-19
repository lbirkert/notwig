use notwig_core::edit::{self, Edge, Selection};
use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{RangeMus, SliceId, TempoMap, TimeMus};

use crate::shell::NotwigUi;
use crate::state::SelectionGroup;
use crate::timeline::interaction::trim_range;
use crate::widgets::snap_time;

use super::actions::apply_slice_edit;
use super::model::SliceModel;
use super::state::{ClipEdge, ClipEdgeDrag, ClipEditorState, SliceDrag, SliceDragMode};

const MICRO: u32 = 1_000_000;

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SliceGeometry {
    pub(super) start: TimeMus,
    pub(super) source_start: u64,
    pub(super) source_end: u64,
}

impl SliceGeometry {
    pub(super) fn duration(self, map: &TempoMap) -> TimeMus {
        map.samples_to_mus(self.source_end.saturating_sub(self.source_start))
    }
}

fn micro_time(value: f64) -> TimeMus {
    signed_time(value.max(0.0))
}

fn signed_time(value: f64) -> TimeMus {
    TimeMus::new((value * f64::from(MICRO)).round() as i64, MICRO)
}

pub fn clamp_slice_start(start: TimeMus, min_start: TimeMus) -> TimeMus {
    start.max(min_start)
}

pub(super) fn local_range(clip: &notwig_core::ClipEntity, range: RangeMus) -> Option<RangeMus> {
    RangeMus::new(range.start - clip.range.start, range.end - clip.range.start).ok()
}

#[allow(clippy::unnecessary_wraps)]
pub fn edge_preview_range(
    drag: ClipEdgeDrag,
    pointer: egui::Pos2,
    ppb: f32,
    grid: TimeMus,
    bypass: bool,
) -> Option<RangeMus> {
    let delta = f64::from(pointer.x - drag.start_pointer.x) / f64::from(ppb);
    let edge = match drag.edge {
        ClipEdge::Start => Edge::Start,
        ClipEdge::End => Edge::End,
    };
    Some(trim_range(
        drag.origin,
        edge,
        delta,
        grid,
        bypass,
        TimeMus::new(1, 16),
    ))
}

pub(super) fn group_starts(
    slices: &[AudioSlice],
    selected: &SelectionGroup<SliceId>,
    drag: Option<SliceDrag>,
) -> Vec<TimeMus> {
    let mut starts: Vec<TimeMus> = slices
        .iter()
        .filter(|slice| selected.contains(&slice.id))
        .map(|slice| slice.start)
        .collect();
    if starts.is_empty()
        && let Some(drag) = drag
    {
        starts.push(drag.origin_start);
    }
    starts
}

fn clamp_slice_group_shift(requested: TimeMus, starts: &[TimeMus], min_start: TimeMus) -> TimeMus {
    let Some(min) = starts.iter().copied().min() else {
        return requested;
    };
    requested.max(min_start - min)
}

pub fn group_move_shift(
    drag: SliceDrag,
    delta_mus: f64,
    grid: TimeMus,
    bypass: bool,
    min_start: TimeMus,
    starts: &[TimeMus],
) -> TimeMus {
    let anchor = clamp_slice_start(
        snap_time(
            micro_time(drag.origin_start.as_f64() + delta_mus),
            grid,
            bypass,
        ),
        min_start,
    );
    clamp_slice_group_shift(anchor - drag.origin_start, starts, min_start)
}

pub(super) fn samples_for_delta(delta: TimeMus, map: &TempoMap) -> i64 {
    map.samples_delta(delta)
}

#[allow(clippy::too_many_arguments)]
pub fn slice_geometry(
    slice: &AudioSlice,
    drag: Option<SliceDrag>,
    delta_mus: f64,
    move_shift: TimeMus,
    map: &TempoMap,
    grid: TimeMus,
    bypass: bool,
    min_start: TimeMus,
    selected: bool,
) -> SliceGeometry {
    let mut geometry = SliceGeometry {
        start: slice.start,
        source_start: slice.source_start_sample,
        source_end: slice.source_end_sample,
    };
    if let Some(drag) = drag {
        if drag.slice_id == slice.id {
            match drag.mode {
                SliceDragMode::Move => {
                    geometry.start = clamp_slice_start(drag.origin_start + move_shift, min_start);
                }
                SliceDragMode::TrimStart => {
                    // A start extension can only pull samples that exist before
                    // the current source window; there is no silent prefix.
                    let source_limit =
                        drag.origin_start - map.samples_to_mus(drag.origin_source_start);
                    let start = clamp_slice_start(
                        snap_time(
                            micro_time(drag.origin_start.as_f64() + delta_mus),
                            grid,
                            bypass,
                        ),
                        min_start.max(source_limit),
                    );
                    let source_delta = samples_for_delta(start - drag.origin_start, map);
                    let source = i128::from(drag.origin_source_start) + i128::from(source_delta);
                    geometry.source_start = u64::try_from(source)
                        .unwrap_or(0)
                        .min(drag.origin_source_end.saturating_sub(1));
                    geometry.start = start;
                }
                SliceDragMode::TrimEnd => {
                    let source_delta = samples_for_delta(signed_time(delta_mus), map);
                    let source = i128::from(drag.origin_source_end) + i128::from(source_delta);
                    let min_end = drag.origin_source_start.saturating_add(1);
                    geometry.source_end = u64::try_from(source).unwrap_or(min_end).max(min_end);
                }
            }
        } else if drag.mode == SliceDragMode::Move && selected {
            geometry.start = clamp_slice_start(slice.start + move_shift, min_start);
        }
    }
    geometry
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_slice_drag(
    ui: &mut NotwigUi,
    state: &mut ClipEditorState,
    audio: &mut AudioClipData,
    slices: Vec<AudioSlice>,
    drag: SliceDrag,
    end: Option<egui::Pos2>,
    map: &TempoMap,
    bypass: bool,
    clip_range: RangeMus,
    ppb: f32,
    grid: TimeMus,
) {
    let delta_mus = end.map_or(0.0, |pos| {
        f64::from(pos.x - drag.origin_pointer.x) / f64::from(ppb)
    });
    let starts = group_starts(&slices, &state.selected, Some(drag));
    let min_start = TimeMus::zero() - clip_range.start;
    let move_shift = group_move_shift(drag, delta_mus, grid, bypass, min_start, &starts);
    let model = SliceModel {
        slices: &slices,
        map: *map,
        clip_length: clip_range.length(),
        clip_start: clip_range.start,
    };
    let plan = match drag.mode {
        SliceDragMode::Move => {
            let objects: Vec<SliceId> = if state.selected.contains(&drag.slice_id) {
                state.selected.iter().copied().collect()
            } else {
                vec![drag.slice_id]
            };
            edit::move_selection(&model, &Selection::objects(objects), move_shift)
        }
        SliceDragMode::TrimStart => {
            let target = clamp_slice_start(
                snap_time(
                    micro_time(drag.origin_start.as_f64() + delta_mus),
                    grid,
                    bypass,
                ),
                min_start,
            );
            edit::trim_object(&model, drag.slice_id, edit::Edge::Start, target)
        }
        SliceDragMode::TrimEnd => {
            let duration = map.samples_to_mus(
                drag.origin_source_end
                    .saturating_sub(drag.origin_source_start),
            );
            let target = signed_time((drag.origin_start + duration).as_f64() + delta_mus);
            edit::trim_object(&model, drag.slice_id, edit::Edge::End, target)
        }
    };
    if let Ok(plan) = plan {
        apply_slice_edit(ui, state, audio, clip_range, map, &slices, plan);
    }
}
