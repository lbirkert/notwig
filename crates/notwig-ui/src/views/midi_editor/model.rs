use std::collections::BTreeSet;

use notwig_core::edit::{
    EditError, EditModel, EditOp, EditorKind, LaneBounds, NextSelection, ObjectExtent, ObjectKind,
    ObjectView, Plan, TrimLimits, epsilon,
};
use notwig_core::{MidiClipData, NoteEntity, NoteId, RangeMus, TimeMus};
use uuid::Uuid;

use crate::intent::UiIntent;
use crate::shell::NotwigUi;

use super::state::MidiEditorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pitch(pub u8);

fn note_span(note: &NoteEntity) -> RangeMus {
    RangeMus::new(note.start, note.start + note.length)
        .unwrap_or_else(|_| RangeMus::new(note.start, note.start).unwrap())
}

pub struct MidiModel<'a> {
    pub(super) midi: &'a MidiClipData,
}

impl<'a> MidiModel<'a> {
    #[must_use]
    pub fn new(midi: &'a MidiClipData) -> Self {
        Self { midi }
    }

    fn note(&self, id: NoteId) -> Option<&NoteEntity> {
        self.midi.notes.iter().find(|note| note.id == id)
    }

    fn span(note: &NoteEntity) -> RangeMus {
        note_span(note)
    }

    fn view(note: &NoteEntity) -> ObjectView<Pitch, NoteId> {
        ObjectView {
            id: note.id,
            lane: Pitch(note.pitch),
            span: Self::span(note),
            kind: ObjectKind::Note,
            extent: ObjectExtent::Span,
        }
    }
}

impl EditModel for MidiModel<'_> {
    type Lane = Pitch;
    type Object = NoteId;
    type Content = NoteEntity;

    fn editor(&self) -> EditorKind {
        EditorKind::Midi
    }

    fn container(&self) -> Option<RangeMus> {
        None
    }

    fn lanes(&self) -> Vec<Pitch> {
        (0..=127).map(Pitch).collect()
    }

    fn lane_bounds(&self, _lane: Pitch) -> LaneBounds {
        LaneBounds {
            start: TimeMus::new(-1_000_000, 1),
            end: None,
        }
    }

    fn objects(&self, lane: Pitch) -> Vec<ObjectView<Pitch, NoteId>> {
        let mut out: Vec<_> = self
            .midi
            .notes
            .iter()
            .filter(|note| note.pitch == lane.0)
            .map(Self::view)
            .collect();
        out.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.id.cmp(&b.id)));
        out
    }

    fn object(&self, object: NoteId) -> Option<ObjectView<Pitch, NoteId>> {
        self.note(object).map(Self::view)
    }

    fn trim_limits(&self, object: NoteId) -> TrimLimits {
        let Some(note) = self.note(object) else {
            return TrimLimits {
                min_start: TimeMus::zero(),
                max_start: TimeMus::zero(),
                min_end: TimeMus::zero(),
                max_end: Some(TimeMus::zero()),
            };
        };
        let span = Self::span(note);
        TrimLimits {
            min_start: TimeMus::new(-1_000_000, 1),
            max_start: span.end - epsilon(),
            min_end: span.start + epsilon(),
            max_end: None,
        }
    }

    fn extract(&self, object: NoteId, window: RangeMus) -> Result<NoteEntity, EditError> {
        let note = self
            .note(object)
            .ok_or_else(|| EditError::NotFound(object.to_string()))?;
        let span = Self::span(note);
        if window.is_empty() || window.start < span.start || window.end > span.end {
            return Err(EditError::InvalidExtraction(
                "window lies outside the note".into(),
            ));
        }
        let mut segment = note.clone();
        segment.start = TimeMus::zero();
        segment.length = window.length();
        Ok(segment)
    }

    fn accepts(&self, _lane: Pitch, kind: ObjectKind) -> bool {
        kind == ObjectKind::Note
    }
}

pub enum NoteSelection {
    Unchanged,
    None,
    Objects(Vec<NoteId>),
    Time(RangeMus),
}

pub struct NoteLowered {
    pub notes: Vec<NoteEntity>,
    pub selection: NoteSelection,
}

fn sort_notes(notes: &mut [NoteEntity]) {
    notes.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(a.pitch.cmp(&b.pitch))
            .then(a.id.cmp(&b.id))
    });
}

fn erase_notes_in_lane(
    notes: &[NoteEntity],
    lane: Pitch,
    span: RangeMus,
    keep: &BTreeSet<NoteId>,
) -> Vec<NoteEntity> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        if note.pitch != lane.0 || keep.contains(&note.id) {
            out.push(note.clone());
            continue;
        }
        let start = note.start;
        let end = note.start + note.length;
        if end <= span.start || start >= span.end {
            out.push(note.clone());
            continue;
        }
        if start >= span.start && end <= span.end {
            continue;
        }
        if start < span.start && end > span.end {
            let mut left = note.clone();
            left.length = span.start - start;
            out.push(left);
            let mut right = note.clone();
            right.id = NoteId::from_uuid(Uuid::new_v4());
            right.start = span.end;
            right.length = end - span.end;
            out.push(right);
        } else if start < span.start {
            let mut left = note.clone();
            left.length = span.start - start;
            out.push(left);
        } else if start < span.end {
            let mut right = note.clone();
            right.start = span.end;
            right.length = end - span.end;
            out.push(right);
        }
    }
    out
}

fn protected_objects(plan: &Plan<Pitch, NoteId, NoteEntity>) -> BTreeSet<NoteId> {
    plan.ops
        .iter()
        .filter_map(|op| match op {
            EditOp::Move { object, .. } | EditOp::Trim { object, .. } => Some(*object),
            _ => None,
        })
        .collect()
}

pub fn lower_note_plan(
    midi_length: TimeMus,
    notes: &[NoteEntity],
    plan: Plan<Pitch, NoteId, NoteEntity>,
) -> NoteLowered {
    let mut out = notes.to_vec();
    let protected = protected_objects(&plan);
    let mut placed: Vec<Option<NoteId>> = vec![None; plan.ops.len()];
    for (index, op) in plan.ops.iter().enumerate() {
        match op {
            EditOp::SetContainer { .. } => {}
            EditOp::Erase { lane, span } => {
                out = erase_notes_in_lane(&out, *lane, *span, &protected);
            }
            EditOp::Remove { object } => {
                out.retain(|note| note.id != *object);
            }
            EditOp::Move {
                object, lane, span, ..
            } => {
                let Some(mut note) = out.iter().find(|note| note.id == *object).cloned() else {
                    continue;
                };
                out.retain(|candidate| candidate.id != *object);
                note.start = span.start;
                note.length = span.length();
                note.pitch = lane.0;
                out = erase_notes_in_lane(&out, *lane, *span, &protected);
                out.push(note);
            }
            EditOp::Trim { object, span } => {
                let Some(mut note) = out.iter().find(|note| note.id == *object).cloned() else {
                    continue;
                };
                let lane = Pitch(note.pitch);
                out.retain(|candidate| candidate.id != *object);
                note.start = span.start;
                note.length = span.length();
                out = erase_notes_in_lane(&out, lane, *span, &protected);
                out.push(note);
            }
            EditOp::Place {
                lane,
                span,
                content,
                ..
            } => {
                out = erase_notes_in_lane(&out, *lane, *span, &protected);
                let mut copy = content.clone();
                copy.id = NoteId::from_uuid(Uuid::new_v4());
                copy.start = span.start;
                copy.length = span.length();
                placed[index] = Some(copy.id);
                out.push(copy);
            }
        }
    }
    let _ = midi_length;
    sort_notes(&mut out);
    let selection = match plan.next {
        NextSelection::Unchanged => NoteSelection::Unchanged,
        NextSelection::None => NoteSelection::None,
        NextSelection::Time(time) => NoteSelection::Time(time.span),
        NextSelection::Objects(refs) => {
            let mut objects = Vec::new();
            for reference in refs {
                match reference {
                    notwig_core::edit::ObjectRef::Existing(id) => {
                        if out.iter().any(|note| note.id == id) {
                            objects.push(id);
                        }
                    }
                    notwig_core::edit::ObjectRef::Created(index) => {
                        if let Some(id) = placed.get(index).and_then(|entry| *entry) {
                            objects.push(id);
                        }
                    }
                }
            }
            NoteSelection::Objects(objects)
        }
    };
    NoteLowered {
        notes: out,
        selection,
    }
}

pub fn apply_note_lowered(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    lowered: NoteLowered,
) {
    let old = midi.notes.clone();
    let removed: Vec<NoteId> = old
        .iter()
        .filter(|note| !lowered.notes.iter().any(|next| next.id == note.id))
        .map(|note| note.id)
        .collect();
    let changed: Vec<NoteEntity> = lowered
        .notes
        .iter()
        .filter(|note| !old.iter().any(|current| current == *note))
        .cloned()
        .collect();
    state.sync_notes(&lowered.notes);
    match &lowered.selection {
        NoteSelection::Unchanged => {
            let live: BTreeSet<NoteId> = lowered.notes.iter().map(|note| note.id).collect();
            state.selected.retain(|id| live.contains(id));
            if !state.selected_is_empty() {
                let ids = state.selected_ids();
                state.select_notes(ids);
            }
        }
        NoteSelection::None => state.clear_selection(),
        NoteSelection::Time(range) => state.select_time(*range),
        NoteSelection::Objects(ids) => state.select_notes(ids.iter().copied()),
    }
    midi.notes.clone_from(&lowered.notes);
    if !changed.is_empty() || !removed.is_empty() {
        ui.emit(UiIntent::EditMidiNotes {
            clip_id: state.clip_id,
            notes: changed,
            removed,
        });
    }
}

/// Insert `note` while clearing (trimming or splitting) any stationary notes
/// that overlap its destination span on the same pitch.
#[must_use]
pub fn insert_note_clearing(notes: &[NoteEntity], note: NoteEntity) -> Vec<NoteEntity> {
    let span = note_span(&note);
    let lane = Pitch(note.pitch);
    let keep: BTreeSet<NoteId> = [note.id].into_iter().collect();
    let mut out = erase_notes_in_lane(notes, lane, span, &keep);
    out.retain(|existing| existing.id != note.id);
    out.push(note);
    sort_notes(&mut out);
    out
}

/// Panel-facing creation helper: inserts a note while clearing its destination
/// span and emits the corresponding create/edit intents.
pub fn create_note_clearing(
    ui: &mut NotwigUi,
    state: &mut MidiEditorState,
    midi: &mut MidiClipData,
    note: NoteEntity,
) {
    let old = midi.notes.clone();
    let next = insert_note_clearing(&old, note.clone());
    let removed: Vec<NoteId> = old
        .iter()
        .filter(|existing| !next.iter().any(|candidate| candidate.id == existing.id))
        .map(|existing| existing.id)
        .collect();
    let changed: Vec<NoteEntity> = next
        .iter()
        .filter(|candidate| candidate.id != note.id)
        .filter(|candidate| !old.iter().any(|current| current == *candidate))
        .cloned()
        .collect();
    midi.notes = next;
    state.sync_notes(&midi.notes);
    state.select_notes([note.id]);
    if !changed.is_empty() || !removed.is_empty() {
        ui.emit(UiIntent::EditMidiNotes {
            clip_id: state.clip_id,
            notes: changed,
            removed,
        });
    }
    ui.emit(UiIntent::CreateMidiNote {
        clip_id: state.clip_id,
        note,
    });
}

/// Default end for notes created with a single click: one grid step, extended
/// until the end is no longer strictly inside an existing note on the pitch.
#[must_use]
pub fn default_note_end(
    start: TimeMus,
    grid: TimeMus,
    pitch: u8,
    notes: &[NoteEntity],
    max_end: TimeMus,
) -> TimeMus {
    let limit = max_end.max(start);
    let mut end = (start + grid).clamp(start, limit);
    for _ in 0..=notes.len() {
        let containing = notes
            .iter()
            .filter(|note| note.pitch == pitch)
            .filter(|note| note.start < end && end < note.start + note.length)
            .min_by_key(|note| note.start);
        let Some(note) = containing else {
            break;
        };
        let next = (note.start + note.length).clamp(end, limit);
        if next <= end {
            break;
        }
        end = next;
    }
    end
}
