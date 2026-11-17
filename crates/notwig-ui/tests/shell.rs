use notwig_core::ids::{Clip, Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{ClipKind, ClipPayload, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::{NotwigUi, RuntimeView, snap_16};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
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
fn hovering_an_audio_file_previews_its_real_length() {
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
    let preview = ui.drag_preview.as_ref().expect("drag preview");
    assert!(preview.duration.as_f64() > 0.0);
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
            type_id: notwig_dsp::stock::EQ_TYPE_ID.into(),
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
