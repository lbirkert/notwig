//! MIDI humanize command.

use notwig_core::Norm01;
use notwig_core::time::TimeMus;
use notwig_ext_api::{
    CommandContext, CommandDescriptor, CommandEdit, CommandEffect, CommandExtension,
};

/// Humanize command id.
pub const HUMANIZE_COMMAND_ID: &str = "notwig.ext.example.humanize";

/// Returns the humanize command metadata.
#[must_use]
pub fn descriptor() -> CommandDescriptor {
    CommandDescriptor {
        id: HUMANIZE_COMMAND_ID.into(),
        title: "Humanize Notes".into(),
        category: "MIDI".into(),
        description: "Nudge the timing and velocity of selected MIDI notes.".into(),
        when: Some("focus == midi_editor && selection.notes > 0".into()),
        default_bindings: vec!["Ctrl+Alt+H".into()],
        extension_id: None,
    }
}

/// Nudges selected note timing and velocity deterministically.
#[derive(Default, notwig_ext_api::ExtensionCall)]
#[extension_call(CommandExtension)]
pub struct HumanizeCommand;

impl CommandExtension for HumanizeCommand {
    fn new() -> Self {
        Self
    }

    fn descriptor() -> CommandDescriptor {
        descriptor()
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

fn note_seed(revision: u64, id: &str) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in revision.to_le_bytes().iter().chain(id.as_bytes()) {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}
