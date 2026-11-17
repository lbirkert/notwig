use notwig_core::ids::{Clip, DeviceChain, Track};
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
