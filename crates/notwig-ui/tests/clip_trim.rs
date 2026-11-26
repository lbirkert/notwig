use egui::Rect;
use notwig_core::ids::{Asset, Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape, MidiClipData};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::test_api::timeline::clip::{clip_base_color, clip_label};
use notwig_ui::{NotwigUi, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn meta() -> Meta {
    Meta {
        name: "Trim".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    }
}

fn audio_clip(name: Option<&str>) -> ClipEntity {
    ClipEntity {
        id: id::<Clip>(60),
        track_id: id::<Track>(2),
        kind: ClipKind::Audio,
        range: RangeMus::new(Rational::new(2, 1), Rational::new(4, 1)).unwrap(),
        name: name.map(str::to_owned),
        color: None,
        muted: false,
        payload: ClipPayload::Audio(AudioClipData {
            asset_id: id::<Asset>(600),
            source_start_sample: 0,
            source_end_sample: 48_000,
            gain_db: Db(0.0),
            gain_curve: None,
            fade_in_samples: 0,
            fade_out_samples: 0,
            fade_shape: FadeShape::Linear,
            slices: Vec::new(),
            slices_enabled: false,
        }),
    }
}

fn midi_clip(name: Option<&str>) -> ClipEntity {
    ClipEntity {
        id: id::<Clip>(61),
        track_id: id::<Track>(2),
        kind: ClipKind::Midi,
        range: RangeMus::new(Rational::zero(), Rational::new(2, 1)).unwrap(),
        name: name.map(str::to_owned),
        color: None,
        muted: false,
        payload: ClipPayload::Midi(MidiClipData {
            length: Rational::new(2, 1),
            content_start: Rational::zero(),
            notes: Vec::new(),
        }),
    }
}

fn audio_doc(clip: &ClipEntity, color: Option<Color>) -> ProjectDoc {
    let doc = ProjectDoc::new_template(meta(), id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: id::<Track>(2),
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color,
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::RegisterAsset {
            asset_id: id::<Asset>(600),
            entry: AssetEntry {
                id: id::<Asset>(600),
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
            id: clip.id,
            track_id: clip.track_id,
            kind: clip.kind,
            range: clip.range,
            payload: clip.payload.clone(),
        }),
    )
    .unwrap()
    .doc
}

#[test]
fn clip_labels_use_the_name_or_a_plain_kind_fallback() {
    assert_eq!(clip_label(&audio_clip(Some("Kick"))), "Kick");
    assert_eq!(clip_label(&audio_clip(None)), "Audio");
    assert_eq!(clip_label(&midi_clip(Some("Bass"))), "Bass");
    assert_eq!(clip_label(&midi_clip(None)), "MIDI");
}

#[test]
fn clip_color_prefers_the_track_color_and_falls_back_to_the_kind() {
    let colored = audio_doc(
        &audio_clip(None),
        Some(Color {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        }),
    );
    let clip = audio_clip(None);
    assert_eq!(
        clip_base_color(&colored, &clip),
        egui::Color32::from_rgb(10, 20, 30)
    );

    let uncolored = audio_doc(&clip, None);
    assert_eq!(
        clip_base_color(&uncolored, &clip),
        notwig_ui::palette::CLIP_AUDIO
    );
}

fn run_ui(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    events: Vec<egui::Event>,
) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    intents
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
fn dragging_an_audio_clip_start_left_extends_the_range_without_moving_the_source() {
    let clip = audio_clip(None);
    let doc = audio_doc(&clip, None);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    let ctx = egui::Context::default();

    let _ = run_ui(&ctx, &mut ui, &doc, &runtime, Vec::new());
    let row = *ui.track_rows.get(&clip.track_id).expect("audio row");
    let ppb = ui.state.viewport.pixels_per_beat;
    let edge = egui::pos2(
        ui.timeline_left + (clip.range.start.as_f64() as f32) * ppb,
        row.top() + 10.0,
    );
    let left = egui::pos2(edge.x - ppb, edge.y);

    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(edge)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(edge, true)],
    );
    let _ = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![egui::Event::PointerMoved(left)],
    );
    let intents = run_ui(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        vec![pointer_event(left, false)],
    );

    let range = intents
        .iter()
        .find_map(|intent| match intent {
            UiIntent::SetClipRange { clip_id, range } if *clip_id == clip.id => Some(*range),
            _ => None,
        })
        .expect("left extension should set the clip range");
    assert_eq!(range.start, TimeMus::new(1, 1));
    assert_eq!(range.end, clip.range.end);
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, UiIntent::EditClipRange { .. }))
    );
}
