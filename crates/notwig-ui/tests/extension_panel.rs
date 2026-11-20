use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notwig_core::ids::{Device, DeviceChain, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_dsp::{DeviceSpec, DeviceUiSpec};
use notwig_ext_api::{
    Color, DeviceDrawContext, DevicePanelHost, DrawCommand, PanelCommand, PanelDraw, Point, Rect,
};
use notwig_ui::{NotwigUi, RuntimeView};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

struct FakeHost {
    called: Arc<AtomicBool>,
    panel: PanelDraw,
}

impl DevicePanelHost for FakeHost {
    fn draw_device_panel(
        &mut self,
        _type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<PanelDraw> {
        self.called.store(true, Ordering::Relaxed);
        assert!(context.width > 0.0);
        assert!(context.height > 0.0);
        Some(self.panel.clone())
    }
}

fn doc() -> ProjectDoc {
    let meta = Meta {
        name: "Extensions".into(),
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
        &Command::new(CommandPayload::InsertDevice {
            id: id::<Device>(50),
            type_id: "com.test.ext".into(),
            version: Some(SchemaVersion::V0),
            kind: DeviceKind::AudioFx,
            target_chain: chain,
            index: None,
            child_chain_ids: Vec::new(),
            name: Some("Ext".into()),
        }),
    )
    .unwrap()
    .doc
}

fn extension_spec() -> DeviceSpec {
    DeviceSpec {
        type_id: "com.test.ext".into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: Vec::new(),
        params: Vec::new(),
        latency_samples: 0,
        name: "Ext".into(),
        vendor: "Test".into(),
        ui: DeviceUiSpec::custom("ext:com.test.ext", Some(300.0)),
    }
}

fn panel() -> PanelDraw {
    PanelDraw {
        commands: vec![PanelCommand::Draw(DrawCommand::FillRect {
            rect: Rect::new(Point::new(0.0, 0.0), Point::new(20.0, 20.0)),
            radius: 2.0,
            color: Color::rgb(255, 0, 0),
        })],
    }
}

fn render(ui: &mut NotwigUi, doc: &ProjectDoc) {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        let _ = ui.show(ctx, doc, &RuntimeView::default());
    });
}

#[test]
fn extension_panel_host_receives_device_context() {
    let doc = doc();
    let called = Arc::new(AtomicBool::new(false));
    let mut ui = NotwigUi::default();
    ui.registry.register(extension_spec());
    ui.state.selection.select_track(id::<Track>(2), false);
    ui.set_device_panel_host(Some(Box::new(FakeHost {
        called: Arc::clone(&called),
        panel: panel(),
    })));
    render(&mut ui, &doc);
    assert!(called.load(Ordering::Relaxed));
}

#[test]
fn invalid_extension_panel_falls_back_without_panicking() {
    let doc = doc();
    let called = Arc::new(AtomicBool::new(false));
    let mut ui = NotwigUi::default();
    ui.registry.register(extension_spec());
    ui.state.selection.select_track(id::<Track>(2), false);
    ui.set_device_panel_host(Some(Box::new(FakeHost {
        called: Arc::clone(&called),
        panel: PanelDraw {
            commands: vec![PanelCommand::Draw(DrawCommand::FillRect {
                rect: Rect::new(Point::new(10.0, 10.0), Point::new(0.0, 0.0)),
                radius: 0.0,
                color: Color::WHITE,
            })],
        },
    })));
    render(&mut ui, &doc);
    assert!(called.load(Ordering::Relaxed));
}
