use notwig_core::Curve;
use notwig_core::Norm01;
use notwig_core::time::TimeMus;
use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorExtension, AudioSubEditorResponse, CommandContext,
    CommandEdit, CommandEffect, CommandExtension, EP_COMMANDS, EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR,
    Extension,
};
use notwig_ext_example::{HUMANIZE_COMMAND_ID, HumanizeCommand, PAN_SUB_EDITOR_ID, PanSubEditor};

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
        clip_range: notwig_core::RangeMus::new(TimeMus::zero(), TimeMus::new(4, 1)).expect("range"),
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
fn runtime_registration_describes_contributions() {
    let manifest: notwig_ext_api::ExtensionManifest =
        serde_json::from_str(include_str!("../extension.json")).expect("valid manifest");
    assert_eq!(manifest.id, "notwig.ext.example");
    assert_eq!(manifest.contributes.len(), 0);

    let mut extension = Extension::new();
    extension.register_extension::<HumanizeCommand>(EP_COMMANDS);
    extension.register_extension::<PanSubEditor>(EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR);

    let registrations = extension.registrations();
    assert_eq!(registrations.commands.len(), 1);
    assert_eq!(registrations.commands[0].id, HUMANIZE_COMMAND_ID);
    assert_eq!(registrations.commands[0].title, "Humanize Notes");
    assert_eq!(registrations.audio_sub_editors.len(), 1);
    assert_eq!(registrations.audio_sub_editors[0].id, PAN_SUB_EDITOR_ID);
    assert_eq!(registrations.audio_sub_editors[0].order, 10);
}

#[test]
fn pan_editor_selection_uses_shared_copy_and_paste() {
    let mut editor = PanSubEditor::new();
    let mut select_context = pan_context();
    select_context.actions.select_all = true;
    draw_pan_editor(&mut editor, &select_context);

    let mut copy_context = pan_context();
    copy_context.actions.copy = true;
    let copied = draw_pan_editor(&mut editor, &copy_context).clipboard;
    assert_eq!(copied.as_ref().map(Vec::len), Some(1));

    let mut paste_context = pan_context();
    paste_context.actions.paste = true;
    paste_context.actions.paste_at = TimeMus::new(2, 1);
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
    extension.register_extension::<PanSubEditor>(EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR);
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

fn command_context() -> CommandContext {
    use std::collections::BTreeMap;

    use notwig_core::{ClipId, NoteEntity, NoteId};
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
    let effect = HumanizeCommand.execute(&command_context());
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
    let mut context = command_context();
    context.selection.notes.clear();
    let effect = HumanizeCommand.execute(&context);
    assert!(matches!(effect, CommandEffect::Error { .. }));
}
