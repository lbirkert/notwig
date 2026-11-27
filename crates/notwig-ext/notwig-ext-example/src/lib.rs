//! Example Notwig extension commands.
//!
//! `notwig.ext.example.humanize` demonstrates the bounded command context,
//! host-owned edit families, and command palette/shortcut integration. It
//! modifies existing selected notes only; the host applies the batch and
//! records one undo entry.
//!
//! `notwig.ext.example.pan` demonstrates an extension-drawn audio-editor
//! sub-editor. The host owns the clip data, context snapshot, validation, and
//! undo boundary; the extension draws the shared curve editor over a waveform
//! underlay and returns host-owned edits.

use notwig_core::Curve;
use notwig_core::Norm01;
use notwig_core::time::TimeMus;
use notwig_ext_api::{
    AUDIO_SUB_EDITOR_EXTENSION_POINT, AudioSubEditorContext, AudioSubEditorDescriptor,
    AudioSubEditorExtension, AudioSubEditorResponse, COMMANDS_POINT, CommandContext,
    CommandDescriptor, CommandEdit, CommandEffect, CommandExtension, ExtensionBuilder,
    ExtensionPoint, PreloadBuilder, UnloadContext, notwig_ext,
};

/// Humanize command id.
pub const HUMANIZE_COMMAND_ID: &str = "notwig.ext.example.humanize";
/// Clip pan sub-editor id.
pub const PAN_SUB_EDITOR_ID: &str = "notwig.ext.example.pan";
/// Extension-drawn clip pan sub-editor.
pub struct PanSubEditor {
    selected: std::collections::BTreeSet<usize>,
    time_selection: Option<notwig_core::RangeMus>,
    state: notwig_ui::extension_ui::LineEditorState,
}

impl AudioSubEditorExtension for PanSubEditor {
    fn new() -> Self {
        Self {
            selected: std::collections::BTreeSet::new(),
            time_selection: None,
            state: notwig_ui::extension_ui::LineEditorState::default(),
        }
    }

    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse {
        let mut curve = context.clip.pan_curve.clone().unwrap_or_default();
        let edit = notwig_ui::extension_ui::apply_curve_edit_actions(
            &mut curve,
            &mut self.selected,
            &mut self.time_selection,
            &context.actions,
            &context.clipboard,
        );
        let timeline = notwig_ui::extension_ui::TimelineGrid {
            start: context.viewport_start - context.clip_range.start,
            pixels_per_beat: context.pixels_per_beat,
            bar_length: context.bar_length,
            beat_length: context.beat_length,
            step: context.grid,
            snap_enabled: context.snap_enabled,
        };
        let config = notwig_ui::extension_ui::LineEditorConfig {
            value_min: -1.0,
            value_max: 1.0,
            baseline: Some(0.0),
            min_time: None,
            max_time: None,
            format_value: Some(format_pan),
        };
        let rect = notwig_ext_api::egui::Rect::from_min_size(
            notwig_ext_api::egui::Pos2::ZERO,
            notwig_ext_api::egui::vec2(context.width, context.height),
        );
        let response = notwig_ui::extension_ui::show_with_background(
            ui,
            rect,
            notwig_ext_api::egui::Id::new((
                "notwig.audio-sub-editor",
                context.editor_id.as_str(),
                context.clip_id,
            )),
            timeline,
            &config,
            &mut curve,
            &mut self.selected,
            &mut self.time_selection,
            &mut self.state,
            &mut |ui, _curve| {
                notwig_ui::extension_ui::draw_waveform_preview(
                    ui.painter(),
                    rect,
                    &context.waveform,
                );
            },
        );
        let effect = if edit.changed || response.committed {
            pan_effect(context, &curve)
        } else {
            CommandEffect::None
        };
        AudioSubEditorResponse {
            effect,
            clipboard: edit.copied,
        }
    }
}

fn pan_effect(context: &AudioSubEditorContext, curve: &Curve) -> CommandEffect {
    CommandEffect::Apply {
        edits: vec![CommandEdit::AudioCurves {
            clip_id: context.clip_id,
            gain_curve: None,
            pan_curve: Some(Some(curve.clone())),
        }],
        ui: Vec::new(),
    }
}

fn format_pan(value: f64) -> String {
    let amount = (value.clamp(-1.0, 1.0).abs() * 100.0).round() as i32;
    if amount == 0 {
        "C".into()
    } else if value < 0.0 {
        format!("L{amount}")
    } else {
        format!("R{amount}")
    }
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

/// Example point used to exercise the preload phase.
pub trait ExamplePoint {}

/// Stable id for the example preload point.
pub const EXAMPLE_POINT: ExtensionPoint<dyn ExamplePoint> =
    ExtensionPoint::new("com.notwig.ext.example.point");

/// Declares example contracts before load-phase registration.
#[notwig_ext(preload)]
pub fn preload(extension: &mut PreloadBuilder) {
    extension.register_extension_point(EXAMPLE_POINT);
}

/// Returns the humanize command metadata.
#[must_use]
pub fn humanize_descriptor() -> CommandDescriptor {
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

/// Returns the clip-pan sub-editor metadata.
#[must_use]
pub fn pan_descriptor() -> AudioSubEditorDescriptor {
    AudioSubEditorDescriptor {
        id: PAN_SUB_EDITOR_ID.into(),
        title: "Clip Pan".into(),
        category: "Audio".into(),
        order: 10,
        extension_id: None,
    }
}

/// Registers the example extension commands.
#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_command::<HumanizeCommand>(COMMANDS_POINT, humanize_descriptor());
    extension.register_extension_audio_sub_editor::<PanSubEditor>(
        AUDIO_SUB_EDITOR_EXTENSION_POINT,
        pan_descriptor(),
    );
}

/// Releases example extension resources.
#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {}

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
    use notwig_ext_api::Extension;

    #[test]
    fn runtime_registration_describes_contributions() {
        let manifest: notwig_ext_api::ExtensionManifest =
            serde_json::from_str(include_str!("../extension.json")).expect("valid manifest");
        assert_eq!(manifest.id, "notwig.ext.example");
        assert_eq!(manifest.contributes.len(), 0);

        let mut extension = Extension::new();
        extension.register_command_with_descriptor::<HumanizeCommand>(humanize_descriptor());
        extension.register_audio_sub_editor_with_descriptor::<PanSubEditor>(pan_descriptor());

        let registrations = extension.registrations();
        assert_eq!(registrations.commands.len(), 1);
        assert_eq!(registrations.commands[0].id, HUMANIZE_COMMAND_ID);
        assert_eq!(registrations.commands[0].title, "Humanize Notes");
        assert_eq!(registrations.audio_sub_editors.len(), 1);
        assert_eq!(registrations.audio_sub_editors[0].id, PAN_SUB_EDITOR_ID);
        assert_eq!(registrations.audio_sub_editors[0].order, 10);
    }

    fn pan_context() -> AudioSubEditorContext {
        use notwig_core::sequencer::AudioClipData;
        use notwig_core::{AssetId, ClipId, Db, FadeShape};
        use uuid::Uuid;

        let audio = AudioClipData {
            asset_id: AssetId::from_uuid(Uuid::from_u128(1)),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            gain_curve: None,
            pan_curve: Some(
                Curve::from_points([notwig_core::CurvePoint::new(TimeMus::zero(), 0.0)])
                    .expect("curve"),
            ),
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        };
        AudioSubEditorContext {
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
            bar_length: TimeMus::new(4, 1),
            beat_length: TimeMus::new(1, 1),
            viewport_start: TimeMus::zero(),
            pixels_per_beat: 80.0,
            width: 400.0,
            height: 200.0,
            waveform: notwig_ext_api::AudioWaveformPreview {
                columns: vec![
                    notwig_ext_api::AudioWaveformColumn {
                        min: -0.5,
                        max: 0.5,
                    };
                    400
                ],
            },
            actions: notwig_ext_api::AudioSubEditorSelectionActions::default(),
            clipboard: Vec::new(),
            theme: notwig_ext_api::ThemeSnapshot::default(),
            input: notwig_ext_api::panel_frame::PanelInput {
                width: 400.0,
                height: 200.0,
                pixels_per_point: 1.0,
                max_texture_side: 4096,
                ..Default::default()
            },
        }
    }

    #[test]
    fn pan_editor_commits_a_host_curve_edit() {
        let context = pan_context();
        let curve = Curve::from_points([notwig_core::CurvePoint::new(TimeMus::zero(), 0.25)])
            .expect("curve");
        let effect = pan_effect(&context, &curve);
        match effect {
            CommandEffect::Apply { edits, .. } => {
                assert!(matches!(edits[0], CommandEdit::AudioCurves { .. }));
            }
            other => panic!("expected an apply effect, got {other:?}"),
        }
    }

    fn draw_pan_editor(
        editor: &mut PanSubEditor,
        context: &AudioSubEditorContext,
    ) -> AudioSubEditorResponse {
        let ctx = notwig_ext_api::egui::Context::default();
        let raw = notwig_ext_api::egui::RawInput {
            screen_rect: Some(notwig_ext_api::egui::Rect::from_min_size(
                notwig_ext_api::egui::Pos2::ZERO,
                notwig_ext_api::egui::vec2(context.width, context.height),
            )),
            ..Default::default()
        };
        let mut response = AudioSubEditorResponse::none();
        let _ = ctx.run(raw, |ctx| {
            notwig_ext_api::egui::CentralPanel::default().show(ctx, |ui| {
                response = editor.draw(ui, context);
            });
        });
        response
    }

    #[test]
    fn pan_editor_selection_uses_shared_copy_and_paste() {
        let mut editor = PanSubEditor::new();
        editor.selected.insert(0);
        let mut context = pan_context();
        context.actions = notwig_ext_api::AudioSubEditorSelectionActions {
            copy: true,
            ..Default::default()
        };
        let copied = draw_pan_editor(&mut editor, &context).clipboard;
        assert_eq!(copied.as_ref().map(Vec::len), Some(1));

        let mut paste_context = context.clone();
        paste_context.actions = notwig_ext_api::AudioSubEditorSelectionActions {
            paste: true,
            paste_at: TimeMus::new(2, 1),
            ..Default::default()
        };
        paste_context.clipboard = copied.expect("copied points");
        let response = draw_pan_editor(&mut editor, &paste_context);
        let CommandEffect::Apply { edits, .. } = response.effect else {
            panic!("expected a pan curve edit");
        };
        let CommandEdit::AudioCurves {
            pan_curve: Some(Some(curve)),
            ..
        } = &edits[0]
        else {
            panic!("expected a pan curve edit");
        };
        assert_eq!(curve.points().len(), 2);
    }

    #[test]
    fn pan_editor_draws_the_curve_and_waveform_underlay() {
        let context = pan_context();
        let mut extension = Extension::new();
        extension.register_audio_sub_editor::<PanSubEditor>(PAN_SUB_EDITOR_ID);
        let handle = extension.create_audio_sub_editor(PAN_SUB_EDITOR_ID);
        assert!(handle > 0);
        let encoded = postcard::to_allocvec(&context).expect("encode context");
        let with_waveform = extension
            .draw_audio_sub_editor_frame(handle, &encoded)
            .expect("draw frame");
        let mut empty_context = context;
        empty_context.waveform = notwig_ext_api::AudioWaveformPreview::default();
        let encoded = postcard::to_allocvec(&empty_context).expect("encode context");
        let without_waveform = extension
            .draw_audio_sub_editor_frame(handle, &encoded)
            .expect("draw frame");
        assert!(
            !with_waveform.panel.primitives.is_empty(),
            "the pan editor should draw the curve editor"
        );
        let vertices = |frame: &notwig_ext_api::AudioSubEditorFrame| {
            frame
                .panel
                .primitives
                .iter()
                .map(|primitive| primitive.mesh.vertices.len())
                .sum::<usize>()
        };
        assert!(
            vertices(&with_waveform) > vertices(&without_waveform),
            "the pan editor should add a waveform underlay"
        );
        extension.destroy_audio_sub_editor(handle);
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
