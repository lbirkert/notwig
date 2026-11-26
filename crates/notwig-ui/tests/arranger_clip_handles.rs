use egui::{Pos2, Rect, Vec2};
use notwig_core::ids::{Asset, Clip, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::sequencer::{AudioClipData, ClipKind, ClipPayload, FadeShape};
use notwig_core::tracks::TrackKind;
use notwig_core::{
    AssetEntry, AssetKind, Db, Id, Meta, ProjectDoc, RangeMus, SampleRate, TimeMus, apply,
};
use notwig_ui::{NotwigUi, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Clip handles".into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    let track = id::<Track>(2);
    let clip = id::<Clip>(60);
    let asset = id::<Asset>(600);
    let doc = ProjectDoc::new_template(meta, id::<Track>(1), id::<DeviceChain>(100)).unwrap();
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::CreateTrack {
            id: track,
            chain_id: id::<DeviceChain>(102),
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    let doc = apply(
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
            id: clip,
            track_id: track,
            kind: ClipKind::Audio,
            range: RangeMus::new(TimeMus::new(1, 1), TimeMus::new(3, 1)).unwrap(),
            payload: ClipPayload::Audio(AudioClipData {
                asset_id: asset,
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
            }),
        }),
    )
    .unwrap()
    .doc
}

fn run(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    time: f64,
    events: Vec<egui::Event>,
) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 900.0))),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            intents = ui.show(ctx, doc, runtime);
        },
    );
    intents
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

#[test]
fn play_start_is_settable_at_the_clip_start_edge() {
    let doc = doc();
    let clip = id::<Clip>(60);
    let runtime = RuntimeView::default();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_clip(clip, id::<Track>(2), false);
    ui.open_clip_editor(clip);
    let ctx = egui::Context::default();
    let _ = run(&ctx, &mut ui, &doc, &runtime, 0.0, Vec::new());
    let content = ui.clip_editor_content_rect;
    let start_x = content.left()
        + (1.0 - ui.state.viewport.start.as_f64()) as f32 * ui.state.viewport.pixels_per_beat;
    let pos = Pos2::new(start_x, content.top() + 7.0);
    let _ = run(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.1,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = run(&ctx, &mut ui, &doc, &runtime, 0.12, vec![button(pos, true)]);
    let intents = run(
        &ctx,
        &mut ui,
        &doc,
        &runtime,
        0.14,
        vec![button(pos, false)],
    );
    assert!(
        intents.iter().any(
            |intent| matches!(intent, UiIntent::SetPlayStart(beat) if *beat == TimeMus::new(1, 1))
        ),
        "the clip edge handle must not block the play start marker"
    );
}
