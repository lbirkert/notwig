use notwig_core::sequencer::{AudioClipData, AudioSlice};
use notwig_core::{RangeMus, SliceId, TempoMap, TimeMus};

use crate::shell::NotwigUi;

const SCROLLBAR_H: f32 = 14.0;

pub(crate) mod actions;
pub(crate) mod geometry;
pub(crate) mod model;
pub(crate) mod navigation;
pub(crate) mod panel;
pub(crate) mod state;

pub use geometry::SliceGeometry;
pub use model::{ClipLane, SliceLowered, SliceModel, SliceSelection};
pub use state::{ClipEdge, ClipEdgeDrag, ClipEditorState, SliceDrag, SliceDragMode};

#[doc(hidden)]
pub mod test_api {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub use super::{
        ClipEdge, ClipLane, SliceDrag, SliceDragMode, SliceGeometry, SliceLowered, SliceModel,
        SliceSelection,
    };

    pub fn lower_slice_plan(
        slices: &[AudioSlice],
        map: &TempoMap,
        plan: notwig_core::edit::Plan<ClipLane, SliceId, AudioSlice>,
    ) -> SliceLowered {
        super::model::lower_slice_plan(slices, map, plan)
    }

    pub fn slice_drag(
        mode: SliceDragMode,
        slice_id: SliceId,
        origin_start: TimeMus,
        origin_source_start: u64,
        origin_source_end: u64,
    ) -> SliceDrag {
        SliceDrag {
            slice_id,
            mode,
            origin_pointer: egui::Pos2::ZERO,
            origin_start,
            origin_source_start,
            origin_source_end,
        }
    }

    pub fn set_drag_origin_start(drag: &mut SliceDrag, origin_start: TimeMus) {
        drag.origin_start = origin_start;
    }

    pub fn drag_origin_start(drag: &SliceDrag) -> TimeMus {
        drag.origin_start
    }

    pub fn clip_edge_drag(
        edge: ClipEdge,
        start_pointer: egui::Pos2,
        origin: RangeMus,
    ) -> ClipEdgeDrag {
        ClipEdgeDrag {
            edge,
            start_pointer,
            origin,
        }
    }

    pub fn group_move_shift(
        drag: SliceDrag,
        delta_mus: f64,
        grid: TimeMus,
        bypass: bool,
        min_start: TimeMus,
        starts: &[TimeMus],
    ) -> TimeMus {
        super::geometry::group_move_shift(drag, delta_mus, grid, bypass, min_start, starts)
    }

    pub fn clamp_slice_start(start: TimeMus, min_start: TimeMus) -> TimeMus {
        super::geometry::clamp_slice_start(start, min_start)
    }

    pub fn edge_preview_range(
        drag: ClipEdgeDrag,
        pointer: egui::Pos2,
        ppb: f32,
        grid: TimeMus,
        bypass: bool,
    ) -> Option<RangeMus> {
        super::geometry::edge_preview_range(drag, pointer, ppb, grid, bypass)
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
        super::geometry::slice_geometry(
            slice, drag, delta_mus, move_shift, map, grid, bypass, min_start, selected,
        )
    }

    pub fn geometry_start(geometry: &SliceGeometry) -> TimeMus {
        geometry.start
    }

    pub fn geometry_source_start(geometry: &SliceGeometry) -> u64 {
        geometry.source_start
    }

    pub fn geometry_source_end(geometry: &SliceGeometry) -> u64 {
        geometry.source_end
    }

    pub fn geometry_duration(geometry: &SliceGeometry, map: &TempoMap) -> TimeMus {
        geometry.duration(map)
    }

    pub fn delete_range(map: &TempoMap, slices: &[AudioSlice], range: RangeMus) -> Vec<AudioSlice> {
        super::model::delete_range(map, slices, range)
    }

    pub fn source_duration(map: &TempoMap, slice: &AudioSlice) -> TimeMus {
        super::model::source_duration(map, slice)
    }

    pub fn slice_model(
        slices: &[AudioSlice],
        map: TempoMap,
        clip_length: TimeMus,
        clip_start: TimeMus,
    ) -> SliceModel<'_> {
        SliceModel {
            slices,
            map,
            clip_length,
            clip_start,
        }
    }

    pub fn emit_slices(
        ui: &mut NotwigUi,
        state: &mut ClipEditorState,
        audio: &mut AudioClipData,
        slices: Vec<AudioSlice>,
    ) {
        super::model::emit_slices(ui, state, audio, slices);
    }

    pub fn playhead_for_strip(runtime: &crate::RuntimeView) -> TimeMus {
        super::panel::playhead_for_strip(runtime)
    }

    pub fn event_color(
        doc: &notwig_core::ProjectDoc,
        clip: &notwig_core::ClipEntity,
    ) -> egui::Color32 {
        super::panel::event_color(doc, clip)
    }

    pub fn handle_clip_editor_actions(
        ui: &mut NotwigUi,
        ctx: &egui::Context,
        state: &mut ClipEditorState,
        audio: &mut AudioClipData,
        clip: &notwig_core::ClipEntity,
        map: &TempoMap,
        clipboard: &crate::Clipboard,
        playhead: TimeMus,
    ) {
        super::actions::handle_clip_editor_actions(
            ui, ctx, state, audio, clip, map, clipboard, playhead,
        );
    }
}
