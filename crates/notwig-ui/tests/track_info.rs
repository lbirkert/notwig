use notwig_core::ids::{DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_ui::{MeterValues, NotwigUi, RuntimeView};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

fn doc_with_track() -> (ProjectDoc, TrackId) {
    let meta = Meta {
        name: "Track Info".into(),
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
    (doc, track)
}

fn run_frame(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    events: Vec<egui::Event>,
) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 1200.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            ui.show(ctx, doc, runtime);
        },
    );
}

fn set_peak(runtime: &mut RuntimeView, track: TrackId, peak: f32) {
    runtime.meters.insert(
        track,
        MeterValues {
            peak_l: peak,
            peak_r: peak,
            rms_l: peak,
            rms_r: peak,
        },
    );
}

fn click(
    ctx: &egui::Context,
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
    id: egui::Id,
) {
    let rect = ctx.read_response(id).expect("peak label response").rect;
    let pos = rect.center();
    run_frame(ctx, ui, doc, runtime, vec![egui::Event::PointerMoved(pos)]);
    run_frame(
        ctx,
        ui,
        doc,
        runtime,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }],
    );
    run_frame(
        ctx,
        ui,
        doc,
        runtime,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        }],
    );
}

#[test]
fn peak_readout_holds_max_since_reset_and_click_resets_it() {
    let (doc, track) = doc_with_track();
    let mut ui = NotwigUi::default();
    ui.state.selection.select_track(track, false);
    let ctx = egui::Context::default();
    let mut runtime = RuntimeView::default();

    set_peak(&mut runtime, track, 0.5);
    run_frame(&ctx, &mut ui, &doc, &runtime, Vec::new());
    assert_eq!(ui.state.peak_hold.get(&track).copied(), Some(0.5));

    set_peak(&mut runtime, track, 0.1);
    run_frame(&ctx, &mut ui, &doc, &runtime, Vec::new());
    assert_eq!(
        ui.state.peak_hold.get(&track).copied(),
        Some(0.5),
        "the readout should keep the maximum, not the latest block peak"
    );

    let peak_id = egui::Id::new(("sidebar-peak", track));
    click(&ctx, &mut ui, &doc, &runtime, peak_id);
    assert!(
        !ui.state.peak_hold.contains_key(&track),
        "clicking the readout should reset the held peak"
    );

    run_frame(&ctx, &mut ui, &doc, &runtime, Vec::new());
    assert_eq!(ui.state.peak_hold.get(&track).copied(), Some(0.1));
}
