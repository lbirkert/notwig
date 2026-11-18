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
