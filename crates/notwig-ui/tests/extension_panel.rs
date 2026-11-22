use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notwig_core::ids::{Device, DeviceChain, Param, Track};
use notwig_core::protocol::{Command, CommandPayload};
use notwig_core::tracks::TrackKind;
use notwig_core::*;
use notwig_dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Unit};
use notwig_ext_api::{
    Color, DeviceDrawContext, DeviceMeterSnapshot, DevicePanelFrame, DevicePanelHost, DrawCommand,
    EguiFrame, EguiImage, EguiMesh, EguiPrimitive, EguiTextureDelta, EguiTextureOptions,
    EguiVertex, PanelAction, PanelCommand, PanelDraw, Point, Rect,
};
use notwig_ui::{NotwigUi, RuntimeView, UiIntent};
use uuid::Uuid;

fn id<T>(n: u128) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(n))
}

struct FakeHost {
    called: Arc<AtomicBool>,
    panel: PanelDraw,
    expected_meter: Option<DeviceMeterSnapshot>,
}

impl DevicePanelHost for FakeHost {
    fn draw_device_panel(
        &mut self,
        _type_id: &str,
        context: &DeviceDrawContext,
    ) -> Option<DevicePanelFrame> {
        self.called.store(true, Ordering::Relaxed);
        assert!(context.width > 0.0);
        assert!(context.height > 0.0);
        if let Some(expected) = self.expected_meter {
            assert_eq!(context.meter, expected);
        }
        Some(DevicePanelFrame::Commands(self.panel.clone()))
    }
}

struct FakeEguiHost {
    action: Option<PanelAction>,
}

impl DevicePanelHost for FakeEguiHost {
    fn draw_device_panel(
        &mut self,
        _type_id: &str,
        _context: &DeviceDrawContext,
    ) -> Option<DevicePanelFrame> {
        Some(DevicePanelFrame::Egui(EguiFrame {
            primitives: vec![EguiPrimitive {
                clip_rect: [0.0, 0.0, 200.0, 200.0],
                mesh: EguiMesh {
                    indices: vec![0, 1, 2, 0, 2, 3],
                    vertices: vec![
                        EguiVertex {
                            pos: [0.0, 0.0],
                            uv: [0.0, 0.0],
                            color: [255; 4],
                        },
                        EguiVertex {
                            pos: [20.0, 0.0],
                            uv: [1.0, 0.0],
                            color: [255; 4],
                        },
                        EguiVertex {
                            pos: [20.0, 20.0],
                            uv: [1.0, 1.0],
                            color: [255; 4],
                        },
                        EguiVertex {
                            pos: [0.0, 20.0],
                            uv: [0.0, 1.0],
                            color: [255; 4],
                        },
                    ],
                    texture_id: 0,
                },
            }],
            textures: vec![EguiTextureDelta {
                id: 0,
                pos: None,
                options: EguiTextureOptions {
                    linear: true,
                    repeat: false,
                },
                image: EguiImage {
                    width: 1,
                    height: 1,
                    pixels: vec![u32::MAX],
                },
            }],
            free_textures: Vec::new(),
            actions: self.action.clone().into_iter().collect(),
            cursor: None,
            wants_keyboard: false,
            wants_pointer: false,
            copy_text: None,
            repaint_after_ms: None,
        }))
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
        actions: Vec::new(),
    }
}

fn render(ui: &mut NotwigUi, doc: &ProjectDoc) -> Vec<UiIntent> {
    render_with_runtime(ui, doc, &RuntimeView::default())
}

fn render_with_runtime(
    ui: &mut NotwigUi,
    doc: &ProjectDoc,
    runtime: &RuntimeView,
) -> Vec<UiIntent> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = ctx.run(input, |ctx| {
        intents = ui.show(ctx, doc, runtime);
    });
    intents
}

fn extension_spec_with_param(param_id: ParamId) -> DeviceSpec {
    let mut spec = extension_spec();
    spec.params.push(ParamSpec::continuous(
        param_id,
        "Gain",
        Unit::Decibels,
        -24.0,
        24.0,
        0.0,
    ));
    spec
}

#[test]
fn egui_frame_composites_and_applies_actions() {
    let doc = doc();
    let param_id = id::<Param>(7);
    let mut ui = NotwigUi::default();
    ui.registry.register(extension_spec_with_param(param_id));
    ui.state.selection.select_track(id::<Track>(2), false);
    ui.set_device_panel_host(Some(Box::new(FakeEguiHost {
        action: Some(PanelAction::SetParamNormalized {
            param_id,
            norm: 0.5,
        }),
    })));
    let intents = render(&mut ui, &doc);
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::SetParam {
            device_id,
            param_id: param,
            value,
        } if *device_id == id::<Device>(50)
            && *param == param_id
            && *value == ParamValue::F64(0.0)
    )));
}

#[test]
fn panel_action_sets_param_normalized() {
    let doc = doc();
    let param_id = id::<Param>(7);
    let mut ui = NotwigUi::default();
    ui.registry.register(extension_spec_with_param(param_id));
    ui.state.selection.select_track(id::<Track>(2), false);
    ui.set_device_panel_host(Some(Box::new(FakeHost {
        called: Arc::new(AtomicBool::new(false)),
        panel: PanelDraw {
            commands: Vec::new(),
            actions: vec![PanelAction::SetParamNormalized {
                param_id,
                norm: 0.5,
            }],
        },
        expected_meter: None,
    })));
    let intents = render(&mut ui, &doc);
    assert!(intents.iter().any(|intent| matches!(
        intent,
        UiIntent::SetParam {
            device_id,
            param_id: param,
            value,
        } if *device_id == id::<Device>(50)
            && *param == param_id
            && *value == ParamValue::F64(0.0)
    )));
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
        expected_meter: None,
    })));
    let _ = render(&mut ui, &doc);
    assert!(called.load(Ordering::Relaxed));
}

#[test]
fn extension_panel_receives_device_meter() {
    let doc = doc();
    let meter = DeviceMeterSnapshot {
        input_peak_l: 0.5,
        input_peak_r: 0.25,
        output_peak_l: 0.125,
        output_peak_r: 0.0625,
    };
    let called = Arc::new(AtomicBool::new(false));
    let mut ui = NotwigUi::default();
    ui.registry.register(extension_spec());
    ui.state.selection.select_track(id::<Track>(2), false);
    ui.set_device_panel_host(Some(Box::new(FakeHost {
        called: Arc::clone(&called),
        panel: panel(),
        expected_meter: Some(meter),
    })));
    let mut runtime = RuntimeView::default();
    runtime.device_meters.insert(id::<Device>(50), meter);
    let _ = render_with_runtime(&mut ui, &doc, &runtime);
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
            actions: Vec::new(),
        },
        expected_meter: None,
    })));
    let _ = render(&mut ui, &doc);
    assert!(called.load(Ordering::Relaxed));
}
