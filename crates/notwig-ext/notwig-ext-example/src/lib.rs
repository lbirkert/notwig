//! Example Notwig extension commands.
//!
//! `notwig.ext.example.humanize` demonstrates the bounded command context,
//! host-owned edit families, and command palette/shortcut integration. It
//! modifies existing selected notes only; the host applies the batch and
//! records one undo entry.

use notwig_core::Norm01;
use notwig_core::time::TimeMus;
use notwig_ext_api::{
    CommandContext, CommandEdit, CommandEffect, CommandExtension, Extension, notwig_extension,
};

/// Humanize command id. This must match `extension.json`.
pub const HUMANIZE_COMMAND_ID: &str = "notwig.ext.example.humanize";

/// Nudges selected note timing and velocity deterministically.
pub struct HumanizeCommand;

impl CommandExtension for HumanizeCommand {
    fn new() -> Self {
        Self
    }

    fn execute(&mut self, context: &CommandContext) -> CommandEffect {
        let Some(clip_id) = context.active_clip else {
            return CommandEffect::Error {
                message: "Open a MIDI clip before running Humanize Notes.".into(),
            };
        };
        if context.selection.notes.is_empty() {
            return CommandEffect::Error {
                message: "Select MIDI notes to humanize.".into(),
            };
        }
        let notes = context
            .selection
            .notes
            .iter()
            .map(|note| {
                let mut note = note.clone();
                let seed = note_seed(context.revision, note.id.to_string().as_str());
                let timing = i64::from(seed % 5) - 2;
                let velocity = (f64::from(seed >> 8) / f64::from(u32::MAX)) - 0.5;
                let shifted = note.start + TimeMus::new(timing, 96);
                note.start = shifted.max(TimeMus::zero());
                let value = f64::from(note.velocity.0) + velocity * 0.16;
                note.velocity = Norm01(value.clamp(0.01, 1.0) as f32);
                note
            })
            .collect();
        CommandEffect::Apply {
            edits: vec![CommandEdit::MidiNotes {
                clip_id,
                notes,
                removed: Vec::new(),
            }],
            ui: Vec::new(),
        }
    }
}

/// Registers the example extension commands.
#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_command::<HumanizeCommand>(HUMANIZE_COMMAND_ID);
}

fn note_seed(revision: u64, id: &str) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in revision.to_le_bytes().iter().chain(id.as_bytes()) {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    fn context() -> CommandContext {
        use std::collections::BTreeMap;

        use notwig_core::{ClipId, NoteEntity, NoteId, TimeMus};
        use uuid::Uuid;
        let note = NoteEntity {
            id: NoteId::from_uuid(Uuid::from_u128(7)),
            pitch: 60,
            start: TimeMus::new(1, 4),
            length: TimeMus::new(1, 4),
            velocity: Norm01(0.5),
            release_velocity: None,
            expression: BTreeMap::default(),
        };
        CommandContext {
            revision: 3,
            focus: notwig_ext_api::CommandFocus::MidiEditor,
            selection: notwig_ext_api::CommandSelection {
                notes: vec![note],
                ..Default::default()
            },
            active_clip: Some(ClipId::from_uuid(Uuid::from_u128(9))),
            ..Default::default()
        }
    }

    #[test]
    fn humanize_returns_note_edits() {
        let effect = HumanizeCommand.execute(&context());
        let CommandEffect::Apply { edits, .. } = effect else {
            panic!("expected an apply effect");
        };
        assert_eq!(edits.len(), 1);
        let CommandEdit::MidiNotes { notes, .. } = &edits[0] else {
            panic!("expected note edits");
        };
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn humanize_requires_midi_context() {
        let mut context = context();
        context.selection.notes.clear();
        let effect = HumanizeCommand.execute(&context);
        assert!(matches!(effect, CommandEffect::Error { .. }));
    }
}
