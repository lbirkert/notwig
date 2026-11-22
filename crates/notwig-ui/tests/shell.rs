use notwig_core::ids::{Asset, Clip, Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::{AudioWaveform, NotwigUi, RuntimeView, UiIntent, snap_16};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn open_clip_editor(ui: &mut NotwigUi, doc: &ProjectDoc, clip: ClipId) {
    if let Some(clip) = doc.arranger.clips.get(&clip) {
        ui.state
            .selection
            .select_clip(clip.id, clip.track_id, false);
    }
    ui.open_clip_editor(clip);
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Shell".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let track = id::<Track>(2);
    let chain = id::<DeviceChain>(102);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
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
    apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(20),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::zero(), Rational::new(4, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(4, 1),
                content_start: Rational::zero(),
                notes: vec![],
            }),
        }),
    )
    .unwrap()
    .doc
}

fn audio_doc() -> ProjectDoc {
    let mut doc = doc();
    let track = id::<Track>(3);
    let chain = id::<DeviceChain>(103);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let asset = id::<Asset>(600);
    doc = apply(
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
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap()
    .doc;
    apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(60),
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: vec![notwig_core::sequencer::AudioSlice {
                    id: notwig_core::SliceId::from_uuid(Uuid::from_u128(77)),
                    start: Rational::zero(),
                    source_start_sample: 0,
                    source_end_sample: 48_000,
                    silent_prefix_samples: 0,
                    gain_db: Db(0.0),
                    asset_id: None,
                }],
                slices_enabled: true,
            }),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn cutting_clip_editor_bar_selection_from_ui_emits_slices() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut runtime = RuntimeView::default();
    runtime.insert_waveform(
        id::<Asset>(600),
        AudioWaveform::from_samples(vec![0.5; 128]),
    );
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let run =
        |ui: &mut NotwigUi, events: Vec<egui::Event>, intents: &mut Vec<notwig_ui::UiIntent>| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    modifiers: egui::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    events,
                    ..Default::default()
                },
                |ctx| {
                    *intents = ui.show(ctx, &doc, &runtime);
                },
            );
        };
    let mut intents = Vec::new();
    run(&mut ui, Vec::new(), &mut intents);
    let content = ui.clip_editor_content_rect;
    assert!(content.width() > 0.0);
    let y = content.top() + 80.0;
    let start = egui::pos2(content.left() + 80.0, y);
    let end = egui::pos2(content.left() + 160.0, y);
    run(
        &mut ui,
        vec![egui::Event::PointerMoved(start)],
        &mut intents,
    );
    run(
        &mut ui,
        vec![
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerMoved(start),
        ],
        &mut intents,
    );
    run(&mut ui, vec![egui::Event::PointerMoved(end)], &mut intents);
    run(
        &mut ui,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        }],
        &mut intents,
    );
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
            events: vec![egui::Event::Key {
                key: egui::Key::X,
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
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let slices = intents
        .iter()
        .find_map(|intent| match intent {
            notwig_ui::UiIntent::SetClipSlices { slices, .. } => Some(slices.clone()),
            _ => None,
        })
        .expect("bar cut should emit slices");
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].start, Rational::zero());
    assert_eq!(slices[0].source_start_sample, 0);
    assert_eq!(slices[0].source_end_sample, 24_000);
}

#[test]
fn clicking_the_timeline_reclaims_shortcut_focus_from_the_clip_editor() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    ui.edit_focus = notwig_ui::EditFocus::ClipEditor;
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView::default();
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let row = *ui.track_rows.get(&id::<Track>(3)).expect("audio row");
    let point = egui::pos2(ui.timeline_left + 10.0, row.top() + 10.0);
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert_eq!(ui.edit_focus, notwig_ui::EditFocus::Timeline);
    assert!(ui.state.selection.clips.contains(&clip));
    let _ = intents;

    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            modifiers: egui::Modifiers {
                command: true,
                ..Default::default()
            },
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        notwig_ui::UiIntent::CopySelection { clips, .. } if clips.contains(&clip)
    )));
}

#[test]
fn dragging_clip_editor_start_marker_emits_a_range_edit() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView::default();
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let content = ui.clip_editor_content_rect;
    let start = egui::pos2(content.left(), content.top() + 15.0);
    let end = egui::pos2(content.left() + 80.0, content.top() + 15.0);
    let mut run = |events: Vec<egui::Event>, intents: &mut Vec<notwig_ui::UiIntent>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ctx| {
                *intents = ui.show(ctx, &doc, &runtime);
            },
        );
    };
    run(vec![egui::Event::PointerMoved(start)], &mut intents);
    run(
        vec![
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerMoved(start),
        ],
        &mut intents,
    );
    run(
        vec![egui::Event::PointerMoved(egui::pos2(
            f32::midpoint(start.x, end.x),
            start.y,
        ))],
        &mut intents,
    );
    run(vec![egui::Event::PointerMoved(end)], &mut intents);
    run(
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        }],
        &mut intents,
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        notwig_ui::UiIntent::SetClipRange { range, .. }
            if range.start == Rational::new(1, 1) && range.end == Rational::new(2, 1)
    )));
}

#[test]
fn finder_audio_drop_targets_the_clip_editor() {
    use notwig_ui::{DropAudioTarget, UiIntent};
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut runtime = RuntimeView::default();
    runtime.insert_waveform(
        id::<Asset>(600),
        AudioWaveform::from_samples(vec![0.5; 128]),
    );
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let content = ui.clip_editor_content_rect;
    let point = egui::pos2(content.left() + 80.0, content.top() + 80.0);
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../notwig-project/tests/fixtures/tone.mp3");
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            hovered_files: vec![egui::HoveredFile {
                path: Some(fixture),
                ..Default::default()
            }],
            events: vec![egui::Event::PointerMoved(point)],
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let preview = ui.drag_preview.as_ref().expect("clip editor drag preview");
    assert!(matches!(
        preview.target,
        DropAudioTarget::ClipEditor { clip_id } if clip_id == clip
    ));
    assert_eq!(preview.at, Rational::new(1, 1));

    let file = std::path::PathBuf::from("/tmp/demo.wav");
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            dropped_files: vec![egui::DroppedFile {
                path: Some(file),
                ..Default::default()
            }],
            events: vec![egui::Event::PointerMoved(point)],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::DropAudio {
            target: DropAudioTarget::ClipEditor { clip_id },
            at,
            ..
        } if *clip_id == clip && *at == Rational::new(1, 1)
    )));
}

#[test]
fn ui_renders_headless_without_panicking() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let mut frames = 0;
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let intents = ui.show(ctx, &doc, &runtime);
            assert_eq!(intents.len(), 0);
            frames += 1;
        });
    }
    assert!(frames >= 2);
}

fn collect_text(shape: &egui::Shape, texts: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => texts.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn footer_chrome_drops_workspace_labels_and_inert_icons() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = clip_editor_screen();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    for removed in ["ARRANGE", "MIX", "EDIT", "CLICK"] {
        assert!(
            !texts.iter().any(|text| text == removed),
            "{removed} should no longer render in the footer"
        );
    }
    assert!(
        texts.iter().any(|text| text.contains("Device Editor")),
        "the device editor action should still render"
    );
    assert!(
        texts.iter().any(|text| text == "FILE"),
        "the top bar should still render"
    );
}

#[test]
fn help_window_lists_commands_and_current_bindings() {
    use notwig_ui::{KeyChord, ShortcutCommand, Shortcuts};

    let doc = doc();
    let mut ui = NotwigUi::default();
    let mut shortcuts = Shortcuts::default();
    shortcuts.assign(
        ShortcutCommand::GroupTracks,
        KeyChord {
            key: egui::Key::G,
            command: false,
            ctrl: false,
            alt: true,
            shift: false,
        },
    );
    ui.set_shortcuts(shortcuts);
    ui.show_help = true;
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    // Run two frames so egui lays out and paints the help window.
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(clip_editor_screen()),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(clip_editor_screen()),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert!(
        texts.iter().any(|text| text.contains("Group Tracks")),
        "help should include Group Tracks"
    );
    assert!(
        texts.iter().any(|text| text.contains("Alt+G")),
        "help should render the current binding"
    );
}

#[test]
fn toolbar_and_footer_use_lucide_glyphs() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = clip_editor_screen();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    for (name, glyph) in [
        ("play", '\u{e13c}'),
        ("stop", '\u{e167}'),
        ("loop", '\u{e146}'),
        ("record", '\u{e076}'),
        ("metronome", '\u{e6bc}'),
        ("info", '\u{e0f9}'),
        ("clip", '\u{e29b}'),
    ] {
        assert!(
            texts.iter().any(|text| text.contains(glyph)),
            "the {name} icon should render its Lucide glyph"
        );
    }
}

#[test]
fn snapping_uses_sixteenth_grid_and_alt_bypasses() {
    let time = Rational::new(1, 3);
    assert_eq!(snap_16(time, false), Rational::new(5, 16));
    assert_eq!(snap_16(time, true), time);
}

#[test]
fn audio_drop_uses_existing_lane_or_creates_track_at_boundary() {
    use notwig_ui::{DropAudioTarget, UiIntent};

    let doc = doc();
    let audio_track = id::<Track>(3);
    let audio_chain = id::<DeviceChain>(103);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: audio_track,
            chain_id: audio_chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let row = *ui.track_rows.get(&audio_track).expect("audio row");
    let file = || egui::DroppedFile {
        path: Some(std::path::PathBuf::from("/tmp/demo.wav")),
        ..Default::default()
    };

    let lane_point = egui::pos2(ui.timeline_left + 90.0, row.center().y);
    let mut lane_intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            dropped_files: vec![file()],
            events: vec![egui::Event::PointerMoved(lane_point)],
            ..Default::default()
        },
        |ctx| {
            lane_intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(lane_intents.iter().any(|intent| matches!(
        intent,
        UiIntent::DropAudio {
            target: DropAudioTarget::Track(track),
            ..
        } if *track == audio_track
    )));

    let boundary_point = egui::pos2(ui.timeline_left + 90.0, row.top() + 2.0);
    let mut boundary_intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            dropped_files: vec![file()],
            events: vec![egui::Event::PointerMoved(boundary_point)],
            ..Default::default()
        },
        |ctx| {
            boundary_intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(boundary_intents.iter().any(|intent| matches!(
        intent,
        UiIntent::DropAudio {
            target: DropAudioTarget::NewTrack { .. },
            ..
        }
    )));
}

#[test]
fn hovering_an_audio_file_probes_its_length_quickly() {
    use notwig_ui::DropAudioTarget;
    let doc = doc();
    let audio_track = id::<Track>(3);
    let audio_chain = id::<DeviceChain>(103);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: audio_track,
            chain_id: audio_chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let row = *ui.track_rows.get(&audio_track).expect("audio row");
    let point = egui::pos2(ui.timeline_left + 90.0, row.center().y);
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../notwig-project/tests/fixtures/tone.mp3");
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            hovered_files: vec![egui::HoveredFile {
                path: Some(fixture.clone()),
                ..Default::default()
            }],
            events: vec![egui::Event::PointerMoved(point)],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let preview = ui.drag_preview.as_ref().expect("drag preview");
    assert!(preview.duration.as_f64() > 0.0);
    assert!(!preview.analyzing);
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::AnalyzeAudioDuration(_)))
    );
    assert!(matches!(
        preview.target,
        DropAudioTarget::Track(track) if track == audio_track
    ));
}

#[test]
fn selecting_a_device_renders_its_info_panel() {
    use notwig_ui::NotwigUi as Ui;
    let doc = doc();
    let audio_track = id::<Track>(3);
    let audio_chain = id::<DeviceChain>(103);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: audio_track,
            chain_id: audio_chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let eq = id::<Device>(400);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: eq,
            type_id: notwig_core::dsp::stock::EQ_TYPE_ID.into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: audio_chain,
            index: None,
            child_chain_ids: vec![],
            name: None,
        }),
    )
    .unwrap()
    .doc;
    let mut ui = Ui::default();
    ui.state.selection.select_device(eq, audio_track, false);
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    assert_eq!(ui.state.selection.primary_device(), Some(eq));
}

#[test]
fn drop_works_from_platform_pointer_without_cursor_events() {
    use notwig_ui::{DropAudioTarget, UiIntent};
    let doc = doc();
    let audio_track = id::<Track>(3);
    let audio_chain = id::<DeviceChain>(103);
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: audio_track,
            chain_id: audio_chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let runtime = RuntimeView::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let row = *ui.track_rows.get(&audio_track).expect("audio row");
    ui.set_platform_pointer(Some(egui::pos2(ui.timeline_left + 120.0, row.center().y)));
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            dropped_files: vec![egui::DroppedFile {
                path: Some(std::path::PathBuf::from("/tmp/demo.mp3")),
                ..Default::default()
            }],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::DropAudio {
            target: DropAudioTarget::Track(track),
            ..
        } if *track == audio_track
    )));
}

#[test]
fn copy_and_paste_events_use_the_current_selection() {
    use notwig_ui::UiIntent;
    let doc = doc();
    let track = doc
        .tracks
        .values()
        .find(|track| track.kind == TrackKind::Instrument)
        .unwrap()
        .id;
    let clip = *doc.arranger.clips.keys().next().unwrap();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, track, false);
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let mut copy_intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ctx| {
            copy_intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(copy_intents.iter().any(|intent| matches!(
        intent,
        UiIntent::CopySelection { clips, time: None } if clips.contains(&clip)
    )));

    let at = Rational::new(6, 1);
    ui.state
        .selection
        .select_time(RangeMus::new(at, at).unwrap(), &[track]);
    let mut paste_intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::Paste("ignored".into())],
            ..Default::default()
        },
        |ctx| {
            paste_intents = ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(
        paste_intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::PasteClips { at: time } if *time == at))
    );
}
#[test]
fn master_track_follows_the_lane_list() {
    let doc = doc();
    let master = doc.master_track().expect("master");
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let master_rect = *ui.track_rows.get(&master).expect("master row");
    let track_rect = *ui.track_rows.get(&id::<Track>(2)).expect("instrument row");
    assert!(
        master_rect.top() >= track_rect.bottom(),
        "master should follow the scrolling lane list"
    );
}
#[test]
fn clip_editor_renders_audio_slices_headlessly() {
    let mut doc = doc();
    let track = id::<Track>(3);
    let chain = id::<DeviceChain>(103);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: chain,
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let asset = id::<Asset>(600);
    doc = apply(
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
                duration_samples: Some(48_000),
                original_sample_rate: Some(48_000),
                original_channels: Some(2),
            },
        }),
    )
    .unwrap()
    .doc;
    let clip = id::<Clip>(60);
    doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateClip {
            id: clip,
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
                source_start_sample: 0,
                source_end_sample: 48_000,
                gain_db: Db(0.0),
                fade_in_samples: 0,
                fade_out_samples: 0,
                fade_shape: FadeShape::Linear,
                slices: Vec::new(),
                slices_enabled: false,
            }),
        }),
    )
    .unwrap()
    .doc;
    let mut runtime = RuntimeView::default();
    runtime.insert_waveform(asset, AudioWaveform::from_samples(vec![0.5; 128]));
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    assert!(ui.clip_editor.is_some());
}

fn run_ui(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    screen: egui::Rect,
    events: Vec<egui::Event>,
) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    intents
}

fn clip_editor_screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0))
}

fn pointer_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

#[test]
fn clip_editor_ruler_updates_global_play_start() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, screen, Vec::new());
    let content = ui.clip_editor_content_rect;
    let ruler = egui::pos2(content.left() + 240.0, content.top() + 15.0);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(ruler)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(ruler, true)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(ruler, false)],
    );
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::SetPlayStart(time) if *time == TimeMus::new(3, 1)
    )));
}

#[test]
fn clip_editor_copy_paste_uses_platform_clipboard_events() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut runtime = RuntimeView::default();
    runtime.insert_waveform(
        id::<Asset>(600),
        AudioWaveform::from_samples(vec![0.5; 128]),
    );
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, screen, Vec::new());
    let content = ui.clip_editor_content_rect;
    let title = egui::pos2(content.left() + 40.0, content.top() + 65.0);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(title)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(title, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(title, false)],
    );
    let slice = notwig_core::SliceId::from_uuid(uuid::Uuid::from_u128(77));
    assert!(
        ui.clip_editor
            .as_ref()
            .is_some_and(|state| state.selected_contains(slice)),
        "event click should select the slice"
    );
    let copy_intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::Copy],
    );
    let clipboard = copy_intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipboard(clipboard) => Some(clipboard.clone()),
            _ => None,
        })
        .expect("copy should publish an extract to the shared clipboard");
    runtime.clipboard = clipboard;
    runtime.playhead = TimeMus::new(3, 1);
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::Paste(String::new())],
    );
    let slices = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipSlices { slices, .. } => Some(slices.clone()),
            _ => None,
        })
        .expect("paste should emit slices");
    assert!(slices.iter().any(|slice| slice.start == TimeMus::new(3, 1)));
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetClipRange { .. }))
    );
}

#[test]
fn clip_editor_event_body_moves_outside_the_clip() {
    let doc = audio_doc();
    let clip = id::<Clip>(60);
    let mut runtime = RuntimeView::default();
    runtime.insert_waveform(
        id::<Asset>(600),
        AudioWaveform::from_samples(vec![0.5; 128]),
    );
    let mut ui = NotwigUi::default();
    open_clip_editor(&mut ui, &doc, clip);
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, screen, Vec::new());
    let content = ui.clip_editor_content_rect;
    let body = egui::pos2(content.left() + 40.0, content.top() + 65.0);
    let body_end = egui::pos2(content.left() + 400.0, body.y);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(body)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(body, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(body_end)],
    );
    let body_intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(body_end, false)],
    );
    assert!(
        !body_intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetClipSlices { .. })),
        "dragging the event body must not move the event"
    );

    let handle = egui::pos2(content.left() + 40.0, content.top() + 53.0);
    let end = egui::pos2(content.left() + 400.0, handle.y);
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(handle)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(handle, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![egui::Event::PointerMoved(end)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        screen,
        vec![pointer_event(end, false)],
    );
    let slices = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipSlices { slices, .. } => Some(slices.clone()),
            _ => None,
        })
        .expect("title handle drag should emit slices");
    assert!(slices.iter().any(|slice| slice.start > TimeMus::new(2, 1)));
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::SetClipRange { .. }))
    );
}

#[test]
fn settings_page_renders_audio_options() {
    use notwig_ui::{
        AudioChannelConfig, AudioChannelDirection, AudioChannelFormat, AudioDeviceView,
        AudioDriverView, AudioSettingsValue, AudioSettingsView,
    };
    let runtime = RuntimeView {
        audio_settings: AudioSettingsView {
            drivers: vec![AudioDriverView {
                id: "macOS Audio".into(),
                label: "macOS Audio".into(),
                devices: vec![AudioDeviceView {
                    id: "System Out".into(),
                    label: "System Out".into(),
                    input_channels: 2,
                    output_channels: 2,
                    sample_rates: vec![44_100, 48_000, 96_000],
                }],
            }],
            current: AudioSettingsValue {
                driver: "macOS Audio".into(),
                device: "System Out".into(),
                sample_rate: "auto".into(),
                block_size: 512,
                channels: vec![
                    AudioChannelConfig {
                        id: "voice".into(),
                        name: "Voice".into(),
                        direction: AudioChannelDirection::Input,
                        format: AudioChannelFormat::Mono,
                        driver: "macOS Audio".into(),
                        device: "System Out".into(),
                        left_channel: 0,
                        right_channel: None,
                    },
                    AudioChannelConfig {
                        id: "main".into(),
                        name: "Main Out".into(),
                        direction: AudioChannelDirection::Output,
                        format: AudioChannelFormat::Stereo,
                        driver: "macOS Audio".into(),
                        device: "System Out".into(),
                        left_channel: 0,
                        right_channel: None,
                    },
                ],
                master_output: Some("main".into()),
            },
            error: None,
            warning: None,
        },
        ..RuntimeView::default()
    };
    let mut ui = NotwigUi::default();
    ui.settings_open = true;
    ui.audio_draft = runtime.audio_settings.current.clone();
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc(), &runtime);
        },
    );
    assert!(ui.settings_open);
}

#[test]
fn recording_take_renders_on_the_armed_lane() {
    use notwig_ui::RecordingTakeView;
    let doc = audio_doc();
    let runtime = RuntimeView {
        recording_takes: vec![RecordingTakeView {
            track_id: id::<Track>(3),
            start: Rational::zero(),
            end: Rational::new(2, 1),
            waveform: std::sync::Arc::new(AudioWaveform::from_samples(vec![0.5; 256])),
        }],
        ..RuntimeView::default()
    };
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
}

#[test]
fn dsp_analysis_window_renders_history_and_stats() {
    use notwig_ui::{DspAnalysisView, RuntimeView};
    let runtime = RuntimeView {
        cpu_load: Some(42.0),
        dsp: DspAnalysisView {
            history: (0..200).map(|index| (index % 100) as f32).collect(),
            block_size: 2048,
            sample_rate: 48_000,
            deadline_ms: 42.667,
            load_avg_ms: 0.154,
            load_max_ms: 0.608,
            period_jitter_percent: 100.0,
            input_latency_samples: 2882,
            output_latency_samples: 2178,
            timing: "Device".into(),
        },
        ..RuntimeView::default()
    };
    let mut ui = NotwigUi::default();
    ui.dsp_open = true;
    let ctx = egui::Context::default();
    let screen = clip_editor_screen();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc(), &runtime);
        },
    );
    assert!(ui.dsp_open);
}

#[test]
fn render_progress_and_notification_overlays_render() {
    let mut ui = NotwigUi::default();
    ui.render_progress = Some(0.42);
    ui.state.notify(
        notwig_ui::NotificationKind::Info,
        "Rendering audio".to_string(),
    );
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(clip_editor_screen()),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc(), &RuntimeView::default());
        },
    );
    assert_eq!(ui.render_progress, Some(0.42));
    assert_eq!(ui.state.notifications.len(), 1);
}

#[test]
fn app_bar_renders_brand_title_and_saved_project_name() {
    let doc = doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let mut runtime = RuntimeView {
        bundle_path: Some(std::path::PathBuf::from("/tmp/Night Drive.notwig")),
        dirty: true,
        ..RuntimeView::default()
    };
    let mut output = None;
    for _ in 0..2 {
        let frame = ctx.run(
            egui::RawInput {
                screen_rect: Some(clip_editor_screen()),
                ..Default::default()
            },
            |ctx| {
                ui.show(ctx, &doc, &runtime);
            },
        );
        output = Some(frame);
    }
    let output = output.expect("at least one frame rendered");
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert!(
        texts.iter().any(|text| text == "NOTWIG"),
        "the centered app title should render"
    );
    assert!(
        texts.iter().any(|text| text == "Night Drive*"),
        "the project card should use the saved bundle name and dirty marker"
    );
    runtime.dirty = false;
}

#[test]
fn command_a_selects_all_arranger_clips() {
    let doc = audio_doc();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let modifiers = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, &doc, &runtime);
        },
    );
    let clips: Vec<_> = ui.state.selection.clips.iter().copied().collect();
    assert_eq!(clips.len(), 2);
    assert!(clips.contains(&id::<Clip>(20)));
    assert!(clips.contains(&id::<Clip>(60)));
}

#[test]
fn command_j_joins_clips_intersected_by_a_time_selection() {
    let track = id::<Track>(2);
    let doc = apply(
        &doc(),
        &Command::new(CommandPayload::CreateClip {
            id: id::<Clip>(21),
            track_id: track,
            kind: ClipKind::Midi,
            range: RangeMus::new(Rational::new(4, 1), Rational::new(8, 1)).unwrap(),
            payload: ClipPayload::Midi(MidiClipData {
                length: Rational::new(4, 1),
                content_start: Rational::zero(),
                notes: vec![],
            }),
        }),
    )
    .unwrap()
    .doc;
    let mut ui = NotwigUi::default();
    ui.state.selection.select_time(
        RangeMus::new(Rational::new(1, 1), Rational::new(7, 1)).unwrap(),
        &[track],
    );
    let ctx = egui::Context::default();
    let runtime = RuntimeView::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 900.0));
    let modifiers = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(screen),
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::J,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, &doc, &runtime);
        },
    );
    let clips = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::JoinClips { clips } => Some(clips.clone()),
            _ => None,
        })
        .expect("Cmd+J with a time selection should join the intersected clips");
    assert_eq!(clips, vec![id::<Clip>(20), id::<Clip>(21)]);
}
