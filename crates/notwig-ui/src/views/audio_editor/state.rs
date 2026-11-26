use notwig_core::edit::Selection;
use notwig_core::{ClipId, RangeMus, SliceId, TimeMus};

use crate::state::{SelectionGroup, Viewport};
use crate::widgets::line_editor::LineEditorState;

use super::model::ClipLane;

/// Host-owned audio editor surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioSubEditor {
    /// Editable audio events on the clip timeline.
    #[default]
    AudioEvents,
    /// Per-event gain, or clip gain when the clip is not in slice mode.
    Gain,
}

impl AudioSubEditor {
    /// Sub-editors in selector order.
    pub const ALL: [Self; 2] = [Self::AudioEvents, Self::Gain];

    /// Human-readable selector label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AudioEvents => "Audio Events",
            Self::Gain => "Gain",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipEditorState {
    pub clip_id: ClipId,
    pub viewport: Viewport,
    /// Active audio editor surface. Ephemeral UI state.
    pub sub_editor: AudioSubEditor,
    pub(super) selected: SelectionGroup<SliceId>,
    pub(super) bar_selection: Option<RangeMus>,
    pub(super) drag: Option<SliceDrag>,
    pub(super) edge_drag: Option<ClipEdgeDrag>,
    pub(super) preview_range: Option<RangeMus>,
    pub(super) range_start: Option<TimeMus>,
    pub(super) bar_drag: Option<BarDrag>,
    pub(super) line_editor: LineEditorState,
    pub(super) initialized: bool,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceDragMode {
    Move,
    TrimStart,
    TrimEnd,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SliceDrag {
    pub(super) slice_id: SliceId,
    pub(super) mode: SliceDragMode,
    pub(super) origin_pointer: egui::Pos2,
    pub(super) origin_start: TimeMus,
    pub(super) origin_source_start: u64,
    pub(super) origin_source_end: u64,
    pub(super) origin_silent_prefix: u64,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEdge {
    Start,
    End,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct ClipEdgeDrag {
    pub(super) edge: ClipEdge,
    pub(super) start_pointer: egui::Pos2,
    pub(super) origin: RangeMus,
}

#[derive(Debug, Clone)]
pub(super) struct BarDrag {
    pub(super) start_pointer: egui::Pos2,
    pub(super) origin: RangeMus,
    pub(super) slices: Vec<SliceId>,
}

impl ClipEditorState {
    pub fn new(clip_id: ClipId) -> Self {
        Self {
            clip_id,
            viewport: Viewport {
                start: TimeMus::zero(),
                pixels_per_beat: 80.0,
            },
            sub_editor: AudioSubEditor::AudioEvents,
            selected: SelectionGroup::default(),
            bar_selection: None,
            drag: None,
            edge_drag: None,
            preview_range: None,
            range_start: None,
            bar_drag: None,
            line_editor: LineEditorState::default(),
            initialized: false,
        }
    }

    #[doc(hidden)]
    pub fn select_node(&mut self, id: SliceId, additive: bool) {
        if additive {
            self.selected.toggle(id);
        } else {
            self.selected.clear();
            self.bar_selection = None;
            self.selected.select_only(id);
        }
    }

    #[doc(hidden)]
    pub fn select_bar(&mut self, range: RangeMus) {
        self.selected.clear();
        self.bar_selection = Some(range);
    }

    #[doc(hidden)]
    pub fn select_all(&mut self, slices: impl IntoIterator<Item = SliceId>) {
        self.selected.clear();
        self.bar_selection = None;
        for id in slices {
            self.selected.insert(id);
        }
    }

    #[doc(hidden)]
    pub fn selection_mode(&self) -> Option<notwig_core::edit::SelectionMode> {
        let selection = if self.selected.is_empty() {
            self.bar_selection.map_or_else(Selection::default, |range| {
                Selection::time(range, [ClipLane])
            })
        } else {
            Selection::objects(self.selected.iter().copied())
        };
        selection.mode()
    }

    #[doc(hidden)]
    pub fn selected_slice_ids(&self) -> Vec<SliceId> {
        self.selected.iter().copied().collect()
    }

    #[doc(hidden)]
    pub fn selected_is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    #[doc(hidden)]
    pub fn selected_contains(&self, id: SliceId) -> bool {
        self.selected.contains(&id)
    }

    #[doc(hidden)]
    pub fn bar_selection(&self) -> Option<RangeMus> {
        self.bar_selection
    }
}
