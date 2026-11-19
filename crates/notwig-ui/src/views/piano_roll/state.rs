use std::collections::{BTreeMap, BTreeSet};

use notwig_core::edit::{Extract, Selection, TimeSelection};
use notwig_core::{ClipId, Norm01, NoteEntity, NoteId, RangeMus, TimeMus};

use crate::state::{SelectionGroup, Viewport};
use crate::timeline::ClipChromeDrag;

use super::model::Pitch;

#[derive(Debug, Clone)]
pub struct PianoRollState {
    pub clip_id: ClipId,
    pub viewport: Viewport,
    pub pitch_scroll: f32,
    pub(super) selected: SelectionGroup<NoteId>,
    pub(super) time_selection: Option<RangeMus>,
    pub(super) note_spans: BTreeMap<NoteId, RangeMus>,
    pub(super) clipboard: NoteClipboard,
    pub(super) drag: Option<NoteDrag>,
    pub(super) velocity_drag: Option<VelocityDrag>,
    pub(super) chrome_drag: Option<ClipChromeDrag>,
    pub(super) preview_range: Option<RangeMus>,
    pub(super) marquee: Option<MarqueeDrag>,
    pub(super) initialized: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NoteClipboard {
    pub(super) extract: Option<Extract<Pitch, NoteEntity>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteDragMode {
    Move,
    ResizeStart,
    ResizeEnd,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct NoteOrigin {
    pub(super) id: NoteId,
    pub(super) start: TimeMus,
    pub(super) length: TimeMus,
    pub(super) pitch: u8,
}

#[derive(Debug, Clone)]
pub(super) struct NoteDrag {
    pub(super) note_id: NoteId,
    pub(super) mode: NoteDragMode,
    pub(super) origin_pointer: egui::Pos2,
    pub(super) origin_start: TimeMus,
    pub(super) origin_length: TimeMus,
    pub(super) origins: Vec<NoteOrigin>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct VelocityDrag {
    pub(super) note_id: NoteId,
    pub(super) origin_velocity: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MarqueeDrag {
    pub(super) origin_pointer: egui::Pos2,
    pub(super) current_pointer: egui::Pos2,
}

fn all_pitches() -> BTreeSet<Pitch> {
    (0..=127).map(Pitch).collect()
}

impl PianoRollState {
    pub fn new(clip_id: ClipId) -> Self {
        Self {
            clip_id,
            viewport: Viewport {
                start: TimeMus::zero(),
                pixels_per_beat: 80.0,
                scroll_track: 0,
            },
            pitch_scroll: 36.0,
            selected: SelectionGroup::default(),
            time_selection: None,
            note_spans: BTreeMap::new(),
            clipboard: NoteClipboard::default(),
            drag: None,
            velocity_drag: None,
            chrome_drag: None,
            preview_range: None,
            marquee: None,
            initialized: false,
        }
    }

    /// Refresh the note extents used to derive a time range from object
    /// selection. The editor calls this before handling interactions and after
    /// every note edit.
    #[doc(hidden)]
    pub fn sync_notes(&mut self, notes: &[NoteEntity]) {
        self.note_spans = notes
            .iter()
            .map(|note| {
                let span = RangeMus::new(note.start, note.start + note.length)
                    .unwrap_or_else(|_| RangeMus::new(note.start, note.start).unwrap());
                (note.id, span)
            })
            .collect();
    }

    fn recompute_derived_time(&mut self) {
        let mut start: Option<TimeMus> = None;
        let mut end: Option<TimeMus> = None;
        for id in self.selected.iter() {
            let Some(span) = self.note_spans.get(id) else {
                continue;
            };
            start = Some(start.map_or(span.start, |value| value.min(span.start)));
            end = Some(end.map_or(span.end, |value| value.max(span.end)));
        }
        self.time_selection = match (start, end) {
            (Some(start), Some(end)) => RangeMus::new(start, end).ok(),
            _ => None,
        };
    }

    #[doc(hidden)]
    pub fn select_note(&mut self, id: NoteId, additive: bool) {
        if additive {
            self.selected.toggle(id);
        } else {
            self.selected.clear();
            self.selected.select_only(id);
        }
        self.recompute_derived_time();
    }

    #[doc(hidden)]
    pub fn select_notes(&mut self, ids: impl IntoIterator<Item = NoteId>) {
        self.selected.clear();
        for id in ids {
            self.selected.insert(id);
        }
        self.recompute_derived_time();
    }

    #[doc(hidden)]
    pub fn select_time(&mut self, range: RangeMus) {
        self.selected.clear();
        self.time_selection = Some(range);
    }

    #[doc(hidden)]
    pub fn select_marquee(&mut self, ids: impl IntoIterator<Item = NoteId>, range: RangeMus) {
        self.selected.clear();
        for id in ids {
            self.selected.insert(id);
        }
        self.time_selection = Some(range);
    }

    #[doc(hidden)]
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.time_selection = None;
    }

    #[doc(hidden)]
    pub fn selected_is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    #[doc(hidden)]
    pub fn selected_contains(&self, id: NoteId) -> bool {
        self.selected.contains(&id)
    }

    #[doc(hidden)]
    pub fn selected_ids(&self) -> Vec<NoteId> {
        self.selected.iter().copied().collect()
    }

    #[doc(hidden)]
    pub fn time_selection(&self) -> Option<RangeMus> {
        self.time_selection
    }

    #[doc(hidden)]
    pub fn selection_mode(&self) -> Option<notwig_core::edit::SelectionMode> {
        self.selection().mode()
    }

    #[doc(hidden)]
    pub fn selection(&self) -> Selection<Pitch, NoteId> {
        Selection {
            objects: self.selected.iter().copied().collect(),
            time: self.time_selection.map(|span| TimeSelection {
                span,
                lanes: all_pitches(),
            }),
        }
    }

    #[doc(hidden)]
    pub(super) fn default_velocity() -> Norm01 {
        Norm01::clamped(0.8)
    }

    pub(super) fn select_one(&mut self, id: NoteId) {
        self.select_note(id, false);
    }
}
