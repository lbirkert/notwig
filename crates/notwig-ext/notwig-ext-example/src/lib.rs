//! Example Notwig extension commands.
//!
//! `notwig.ext.example.humanize` demonstrates the bounded command context,
//! host-owned edit families, and command palette/shortcut integration. It
//! modifies existing selected notes only; the host applies the batch and
//! records one undo entry.
//!
//! `notwig.ext.example.pan` demonstrates an extension-drawn audio-editor
//! sub-editor. The host owns the clip data, context snapshot, validation, and
//! undo boundary; the extension draws its own panel and returns host-owned
//! edits.

use notwig_core::Norm01;
use notwig_core::time::TimeMus;
use notwig_core::{Curve, CurvePoint};
use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorExtension, CommandContext, CommandEdit, CommandEffect,
    CommandExtension, Extension, notwig_extension,
};

/// Humanize command id. This must match `extension.json`.
pub const HUMANIZE_COMMAND_ID: &str = "notwig.ext.example.humanize";
/// Clip pan sub-editor id. This must match `extension.json`.
pub const PAN_SUB_EDITOR_ID: &str = "notwig.ext.example.pan";
/// Renderer id registered for the clip pan sub-editor.
pub const PAN_RENDERER_ID: &str = "notwig.ext.example.pan_editor";

/// Extension-drawn clip pan sub-editor.
#[derive(Debug, Default)]
pub struct PanSubEditor {
    value: f64,
    committed: Option<f64>,
    initialized: bool,
}

impl AudioSubEditorExtension for PanSubEditor {
    fn new() -> Self {
        Self::default()
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> CommandEffect {
        if !self.initialized {
            self.value = initial_pan(&context.clip);
            self.committed = Some(self.value);
            self.initialized = true;
        }
        let mut changed = false;
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.heading("Clip Pan");
            ui.label("Draws and commits through the shown host edit family.");
            changed = ui
                .add(
                    notwig_ext_api::egui::Slider::new(&mut self.value, -1.0..=1.0)
                        .text("Pan")
                        .show_value(true),
                )
                .changed();
        });
        let released = ui.input(|input| !input.pointer.primary_down());
        if !changed || !released || self.committed == Some(self.value) {
            return CommandEffect::None;
        }
        self.committed = Some(self.value);
        pan_effect(context, self.value)
    }
}

fn pan_effect(context: &AudioSubEditorContext, value: f64) -> CommandEffect {
    let length = context.clip_range.length();
    let Ok(curve) = Curve::from_points([
        CurvePoint::new(TimeMus::zero(), value),
        CurvePoint::new(length, value),
    ]) else {
        return CommandEffect::None;
    };
    CommandEffect::Apply {
        edits: vec![CommandEdit::AudioCurves {
            clip_id: context.clip_id,
            gain_curve: None,
            pan_curve: Some(Some(curve)),
        }],
        ui: Vec::new(),
    }
}

fn initial_pan(clip: &notwig_ext_api::AudioSubEditorClip) -> f64 {
    clip.pan_curve
        .as_ref()
        .and_then(|curve| curve.points().first().map(|point| point.value))
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0)
}

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
    extension.register_audio_sub_editor::<PanSubEditor>(PAN_RENDERER_ID);
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

    #[test]
    fn pan_renderer_matches_the_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../extension.json")).expect("valid manifest json");
        let contribution = manifest["contributes"]
            .as_array()
            .expect("contributions")
            .iter()
            .find(|entry| entry["id"] == PAN_SUB_EDITOR_ID)
            .expect("pan contribution");
        assert_eq!(
            contribution["point"],
            notwig_ext_api::AUDIO_SUB_EDITOR_POINT
        );
        assert_eq!(
            contribution["contract"],
            notwig_ext_api::AUDIO_SUB_EDITOR_CONTRACT
        );
        assert_eq!(contribution["kind"]["custom"]["renderer"], PAN_RENDERER_ID);
    }

    #[test]
    fn pan_editor_commits_a_host_curve_edit() {
        use notwig_core::sequencer::AudioClipData;
        use notwig_core::{AssetId, ClipId, Db, FadeShape};
        use uuid::Uuid;

        let audio = AudioClipData {
            asset_id: AssetId::from_uuid(Uuid::from_u128(1)),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            gain_curve: None,
            pan_curve: None,
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        };
        let context = AudioSubEditorContext {
            editor_id: PAN_SUB_EDITOR_ID.into(),
            revision: 1,
            clip_id: ClipId::from_uuid(Uuid::from_u128(2)),
            track_id: notwig_core::TrackId::from_uuid(Uuid::from_u128(3)),
            clip_range: notwig_core::RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1))
                .expect("range"),
            clip: audio.into(),
            selected_slices: Vec::new(),
            time_selection: None,
            playhead: TimeMus::zero(),
            loop_range: None,
            tempo_bpm: 120.0,
            sample_rate: 48_000,
            snap_enabled: true,
            grid: TimeMus::new(1, 4),
            viewport_start: TimeMus::zero(),
            pixels_per_beat: 80.0,
            width: 400.0,
            height: 200.0,
            theme: notwig_ext_api::ThemeSnapshot::default(),
            input: notwig_ext_api::panel_frame::PanelInput::default(),
        };
        let effect = pan_effect(&context, 0.25);
        match effect {
            CommandEffect::Apply { edits, .. } => {
                assert!(matches!(edits[0], CommandEdit::AudioCurves { .. }));
            }
            other => panic!("expected an apply effect, got {other:?}"),
        }
    }

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
