use notwig_core::ProjectDoc;
use notwig_core::tracks::TrackKind;
use notwig_ui::test_api::shell::handle_shortcuts;
use notwig_ui::{EditFocus, NotwigUi, OpenedEditor, RuntimeView, UiIntent};

fn clip_editor_doc() -> ProjectDoc {
    let meta = notwig_core::Meta {
        name: "Shell".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: notwig_core::SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(
        meta,
        notwig_core::TrackId::nil(),
        notwig_core::DeviceChainId::nil(),
    )
    .unwrap()
}

fn timeline_doc() -> ProjectDoc {
    let meta = notwig_core::Meta {
        name: "Timeline".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: notwig_core::SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mut doc = ProjectDoc::new_template(
        meta,
        notwig_core::TrackId::nil(),
        notwig_core::DeviceChainId::nil(),
    )
    .unwrap();
    let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let chain = notwig_core::DeviceChainId::from_uuid(uuid::Uuid::from_u128(102));
    doc = notwig_core::apply(
        &doc,
        &notwig_core::protocol::Command::new(notwig_core::protocol::CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Instrument,
            name: "Keys".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    notwig_core::apply(
        &doc,
        &notwig_core::protocol::Command::new(notwig_core::protocol::CommandPayload::CreateClip {
            id: notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20)),
            track_id: track,
            kind: notwig_core::ClipKind::Midi,
            range: notwig_core::RangeMus::new(
                notwig_core::Rational::zero(),
                notwig_core::Rational::new(2, 1),
            )
            .unwrap(),
            payload: notwig_core::ClipPayload::Midi(notwig_core::sequencer::MidiClipData {
                length: notwig_core::Rational::new(2, 1),
                content_start: notwig_core::Rational::zero(),
                notes: Vec::new(),
            }),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn timeline_duplicate_advances_node_selection() {
    let doc = timeline_doc();
    let clip = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20));
    let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, track, false);
    let _ = ctx.run(
        egui::RawInput {
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
            events: vec![egui::Event::Key {
                key: egui::Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(&mut ui, ctx, &doc, &runtime);
        },
    );
    assert!(
        ui.intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::DuplicateSelection { .. }))
    );
}

fn shortcut_intents(
    doc: &ProjectDoc,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Vec<UiIntent> {
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let (clip, track) = doc.arranger.clips.iter().next().map_or(
        (notwig_core::ClipId::nil(), notwig_core::TrackId::nil()),
        |(id, clip)| (*id, clip.track_id),
    );
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, track, false);
    ui.open_clip_editor(clip);
    ui.edit_focus = EditFocus::Timeline;
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(&mut ui, ctx, doc, &runtime);
            intents = std::mem::take(&mut ui.intents);
        },
    );
    intents
}

#[test]
fn timeline_shortcut_family_still_runs_while_clip_editor_is_open() {
    let doc = timeline_doc();
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let copy = shortcut_intents(&doc, vec![egui::Event::Copy], command);
    assert!(matches!(copy.as_slice(), [UiIntent::CopySelection { .. }]));
    let cut = shortcut_intents(&doc, vec![egui::Event::Cut], command);
    assert!(matches!(cut.as_slice(), [UiIntent::CutSelection { .. }]));
    let paste = shortcut_intents(&doc, vec![egui::Event::Paste(String::new())], command);
    assert!(matches!(paste.as_slice(), [UiIntent::PasteClips { .. }]));
    let duplicate = shortcut_intents(
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::D,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: command,
        }],
        command,
    );
    assert!(
        duplicate
            .iter()
            .any(|intent| matches!(intent, UiIntent::DuplicateSelection { .. }))
    );
    let delete = shortcut_intents(
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
        egui::Modifiers::default(),
    );
    assert!(
        delete
            .iter()
            .any(|intent| matches!(intent, UiIntent::DeleteSelection { .. }))
    );
}

#[test]
fn timeline_shortcuts_still_run_while_clip_editor_is_open() {
    let doc = clip_editor_doc();
    let clip = notwig_core::ClipId::nil();
    let track = notwig_core::TrackId::nil();
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, track, false);
    ui.open_clip_editor(clip);
    ui.edit_focus = EditFocus::Timeline;

    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(&mut ui, ctx, &doc, &runtime);
            intents = std::mem::take(&mut ui.intents);
        },
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::CopySelection { clips, .. } if clips.contains(&clip)
    )));
}

#[test]
fn editors_are_mutually_exclusive() {
    let mut ui = NotwigUi::default();
    assert_eq!(ui.currently_opened, Some(OpenedEditor::DeviceEditor));
    ui.open_clip_editor(notwig_core::ClipId::nil());
    assert!(ui.clip_editor.is_some());
    assert_eq!(ui.currently_opened, Some(OpenedEditor::ClipEditor));
    ui.open_device_editor();
    assert!(ui.clip_editor.is_none());
    assert_eq!(ui.currently_opened, Some(OpenedEditor::DeviceEditor));
    ui.close_editor();
    assert_eq!(ui.currently_opened, None);
}

#[test]
fn track_creation_shortcuts_emit_create_track_intents() {
    let doc = timeline_doc();
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let instrument = shortcut_intents(
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: command,
        }],
        command,
    );
    assert!(instrument.iter().any(|intent| matches!(
        intent,
        UiIntent::CreateTrack(notwig_core::TrackKind::Instrument)
    )));

    let shift_command = egui::Modifiers {
        command: true,
        shift: true,
        ..Default::default()
    };
    let audio = shortcut_intents(
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: shift_command,
        }],
        shift_command,
    );
    assert!(
        audio
            .iter()
            .any(|intent| matches!(intent, UiIntent::CreateTrack(notwig_core::TrackKind::Audio)))
    );
}

fn press_command_l(doc: &ProjectDoc, ui: &mut NotwigUi) -> Vec<UiIntent> {
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            modifiers: command,
            events: vec![egui::Event::Key {
                key: egui::Key::L,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            }],
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(ui, ctx, doc, &runtime);
            intents = std::mem::take(&mut ui.intents);
        },
    );
    intents
}

fn mixed_clip_doc() -> ProjectDoc {
    use notwig_core::{
        AssetEntry, AssetKind, AudioClipData, ClipId, ClipKind, ClipPayload, Command,
        CommandPayload, DeviceChainId, FadeShape, Meta, MidiClipData, RangeMus, Rational,
        SampleRate, TrackId,
    };

    let meta = Meta {
        name: "Mixed".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let mut doc = ProjectDoc::new_template(meta, TrackId::nil(), DeviceChainId::nil()).unwrap();
    let midi_track = TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let audio_track = TrackId::from_uuid(uuid::Uuid::from_u128(3));
    for (id, chain, kind, name) in [
        (midi_track, 102, TrackKind::Instrument, "Keys"),
        (audio_track, 103, TrackKind::Audio, "Audio"),
    ] {
        doc = notwig_core::apply(
            &doc,
            &Command::new(CommandPayload::CreateTrack {
                id,
                chain_id: DeviceChainId::from_uuid(uuid::Uuid::from_u128(chain)),
                kind,
                name: name.into(),
                parent: None,
                index: None,
                color: None,
            }),
        )
        .unwrap()
        .doc;
    }
    let asset = notwig_core::AssetId::from_uuid(uuid::Uuid::from_u128(600));
    doc = notwig_core::apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: asset,
            entry: AssetEntry {
                id: asset,
                kind: AssetKind::Audio,
                uri: "assets/test.wav".into(),
                original_name: None,
                hash: None,
                format: "wav".into(),
                sample_rate: Some(48_000),
                channels: Some(2),
                duration_samples: Some(96_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap()
    .doc;
    doc = notwig_core::apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: ClipId::from_uuid(uuid::Uuid::from_u128(20)),
            track_id: midi_track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::new(1, 1), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(1, 1),
                content_start: Rational::zero(),
                notes: Vec::new(),
            }),
        }),
    )
    .unwrap()
    .doc;
    notwig_core::apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: ClipId::from_uuid(uuid::Uuid::from_u128(60)),
            track_id: audio_track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::new(3, 1), Rational::new(7, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 96_000,
                gain_db: notwig_core::Db(0.0),
                gain_curve: None,
                pan_curve: None,
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn loop_selection_uses_selected_clip_bounds() {
    let doc = timeline_doc();
    let clip = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20));
    let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, track, false);

    let intents = press_command_l(&doc, &mut ui);

    assert_eq!(
        intents,
        vec![UiIntent::SetLoopRange(
            notwig_core::RangeMus::new(
                notwig_core::Rational::zero(),
                notwig_core::Rational::new(2, 1),
            )
            .unwrap()
        )]
    );
}

#[test]
fn loop_selection_spans_mixed_midi_and_audio_clips() {
    let doc = mixed_clip_doc();
    let midi = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20));
    let audio = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(60));
    let midi_track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let audio_track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(3));
    let mut ui = NotwigUi::default();
    ui.state
        .selection
        .select_clips(&[(midi, midi_track), (audio, audio_track)]);

    let intents = press_command_l(&doc, &mut ui);

    assert_eq!(
        intents,
        vec![UiIntent::SetLoopRange(
            notwig_core::RangeMus::new(
                notwig_core::Rational::new(1, 1),
                notwig_core::Rational::new(7, 1),
            )
            .unwrap()
        )]
    );
}

#[test]
fn loop_selection_without_a_selection_toggles_the_loop() {
    let doc = timeline_doc();
    let mut ui = NotwigUi::default();

    let intents = press_command_l(&doc, &mut ui);

    assert_eq!(intents, vec![UiIntent::ToggleLoop]);
}

fn device_doc() -> (ProjectDoc, notwig_core::TrackId, notwig_core::DeviceId) {
    let doc = timeline_doc();
    let track = notwig_core::TrackId::from_uuid(uuid::Uuid::from_u128(2));
    let chain = notwig_core::DeviceChainId::from_uuid(uuid::Uuid::from_u128(102));
    let device = notwig_core::DeviceId::from_uuid(uuid::Uuid::from_u128(300));
    let doc = notwig_core::apply(
        &doc,
        &notwig_core::protocol::Command::new(notwig_core::protocol::CommandPayload::InsertDevice {
            id: device,
            type_id: notwig_core::dsp::stock::TOOL_TYPE_ID.into(),
            version: None,
            kind: notwig_core::DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
        }),
    )
    .unwrap()
    .doc;
    (doc, track, device)
}

fn run_shortcut(
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Vec<UiIntent> {
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            handle_shortcuts(ui, ctx, doc, &runtime);
            intents = std::mem::take(&mut ui.intents);
        },
    );
    intents
}

#[test]
fn alt_a_toggles_selected_device_instead_of_its_track() {
    let (doc, track, device) = device_doc();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_device(device, track, false);
    ui.edit_focus = EditFocus::Timeline;
    let modifiers = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let intents = run_shortcut(
        &mut ui,
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        modifiers,
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::SetDevicesActive { device_ids, active: false } if device_ids == &vec![device]
    )));
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetTrackActive { .. }))
    );
}

#[test]
fn alt_a_does_not_toggle_devices_or_tracks_while_an_editor_is_focused() {
    let (doc, track, device) = device_doc();
    let clip = notwig_core::ClipId::from_uuid(uuid::Uuid::from_u128(20));
    let mut ui = NotwigUi::default();
    ui.state.selection.select_device(device, track, false);
    ui.open_midi_editor(clip);
    let modifiers = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let intents = run_shortcut(
        &mut ui,
        &doc,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        modifiers,
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetDevicesActive { .. }))
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetTrackActive { .. }))
    );
}

#[test]
fn device_copy_cut_and_paste_shortcuts_emit_device_intents() {
    let (doc, track, device) = device_doc();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_device(device, track, false);
    ui.edit_focus = EditFocus::Timeline;
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let copy = run_shortcut(&mut ui, &doc, vec![egui::Event::Copy], command);
    assert!(matches!(
        copy.as_slice(),
        [UiIntent::CopyDevices(ids)] if ids == &vec![device]
    ));
    let cut = run_shortcut(&mut ui, &doc, vec![egui::Event::Cut], command);
    assert!(matches!(
        cut.as_slice(),
        [UiIntent::CutDevices(ids)] if ids == &vec![device]
    ));
    let paste = run_shortcut(
        &mut ui,
        &doc,
        vec![egui::Event::Paste(String::new())],
        command,
    );
    assert!(matches!(
        paste.as_slice(),
        [UiIntent::PasteDevices { track_id, index: 1, .. }] if *track_id == track
    ));
}
