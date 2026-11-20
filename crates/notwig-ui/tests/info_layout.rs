use notwig_core::ids::{Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_dsp::{DeviceSpec, DeviceUiSpec};
use notwig_ui::{NotwigUi, RuntimeView};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc_with_track() -> (ProjectDoc, TrackId, DeviceChainId) {
    let meta = Meta {
        name: "Info".into(),
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
            kind: TrackKind::Audio,
            name: "Audio".into(),
            parent: None,
            index: None,
            color: None,
        }),
    )
    .unwrap()
    .doc;
    (doc, track, chain)
}

fn run(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, doc, runtime);
        },
    )
}

fn collect_text(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, out);
            }
        }
        _ => {}
    }
}

#[test]
fn info_panel_keeps_its_configured_width() {
    let (doc, track, _) = doc_with_track();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_track(track, false);
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let _ = run(&ctx, &mut ui, &doc, &runtime);
    let _ = run(&ctx, &mut ui, &doc, &runtime);

    let panel = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("notwig_info"))
        .expect("info panel state");
    assert!(
        (panel.rect.width() - 250.0).abs() < 0.5,
        "info panel should not overflow its exact width, got {}",
        panel.rect.width()
    );
}

#[test]
fn track_info_shows_total_device_chain_latency() {
    let (doc, track, chain) = doc_with_track();
    let mut ui = NotwigUi::default();
    ui.registry.register(DeviceSpec {
        type_id: "test.latency".into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: Vec::new(),
        params: Vec::new(),
        latency_samples: 960,
        name: "Latency Test".into(),
        vendor: "Notwig".into(),
        ui: DeviceUiSpec::Generic,
    });
    let doc = apply(
        &doc,
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(300),
            type_id: "test.latency".into(),
            version: None,
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: Vec::new(),
            name: None,
        }),
    )
    .unwrap()
    .doc;
    ui.state.selection.select_track(track, false);
    let runtime = RuntimeView::default();
    let ctx = egui::Context::default();
    let _ = run(&ctx, &mut ui, &doc, &runtime);
    let output = run(&ctx, &mut ui, &doc, &runtime);

    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut texts);
    }
    assert!(
        texts.iter().any(|text| text == "Chain latency 20 ms"),
        "expected chain latency readout, got {texts:?}"
    );
}
