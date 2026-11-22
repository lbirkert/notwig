use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use notwig_core::{DeviceId, DeviceKind, SchemaVersion, TrackId};
use notwig_dsp::{CompiledPlan, PlanDevice, ProcessContext, RenderConfig, TransportInfo};
use notwig_ext::ExtensionHost;
use notwig_ext_api::{
    DeviceDrawContext, DevicePanelFrame, EguiFrame, EguiImage, EguiMesh, EguiPrimitive,
    EguiTextureDelta, EguiTextureOptions, EguiVertex, PanelAction, PanelCommand, ThemeSnapshot,
};

const DESCRIBE: &str = r#"{"type_id":"com.test.gain","version":{"major":0,"minor":1},"kind":"AudioFx","ports":[],"params":[],"latency_samples":0,"name":"Gain","vendor":"Test"}"#;

const DRAW: &str = r#"{"commands":[{"Draw":{"FillRect":{"rect":{"min":{"x":0.0,"y":0.0},"max":{"x":10.0,"y":10.0}},"radius":2.0,"color":{"r":255,"g":0,"b":0,"a":255}}}}]}"#;

const DRAW_WITH_ACTIONS: &str = r#"{"commands":[{"Draw":{"FillRect":{"rect":{"min":{"x":0.0,"y":0.0},"max":{"x":10.0,"y":10.0}},"radius":2.0,"color":{"r":255,"g":0,"b":0,"a":255}}}}],"actions":["RequestRepaint"]}"#;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn module_wat(draw_json: &str) -> String {
    let describe = escape(DESCRIBE);
    let draw = escape(draw_json);
    let draw_packed = (1024u64 << 32) | draw_json.len() as u64;
    format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 0) "{describe}")
  (data (i32.const 1024) "{draw}")
  (global $next (mut i32) (i32.const 4096))
  (func (export "notwig_abi_version") (result i32) i32.const 2)
  (func (export "notwig_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    global.get $next
    local.set $ptr
    global.get $next
    local.get $len
    i32.add
    global.set $next
    local.get $ptr)
  (func (export "notwig_free") (param i32 i32))
  (func (export "notwig_device_describe") (param i32 i32) (result i64)
    i64.const {describe_len})
  (func (export "notwig_device_create") (param i32 i32) (result i32)
    i32.const 1)
  (func (export "notwig_device_destroy") (param i32))
  (func (export "notwig_device_prepare") (param i32 i32 i32 i32) (result i32)
    i32.const 0)
  (func (export "notwig_device_set_param") (param i32 i32 i32 i32 f64) (result i32)
    i32.const 0)
  (func (export "notwig_device_process")
    (param $handle i32) (param $audio i32) (param $frames i32)
    (param $events i32) (param $event_count i32) (param $pos i64)
    (param $playing i32) (result i32)
    (local $i i32)
    (local $count i32)
    local.get $frames
    i32.const 2
    i32.mul
    local.set $count
    (block $done
      (loop $loop
        local.get $i
        local.get $count
        i32.ge_u
        br_if $done
        local.get $audio
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        local.get $audio
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        f32.load
        f32.const 0.5
        f32.mul
        f32.store
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop))
    i32.const 0)
  (func (export "notwig_device_latency") (param i32) (result i32)
    i32.const 7)
  (func (export "notwig_device_draw") (param i32 i32 i32) (result i64)
    i64.const {draw_packed})
)"#,
        describe_len = DESCRIBE.len(),
    )
}

fn write_extension() -> PathBuf {
    write_extension_with(DRAW)
}

fn write_extension_with(draw_json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-ext-test-{}", uuid::Uuid::new_v4()));
    let extension = dir.join("com.test.gain");
    std::fs::create_dir_all(&extension).unwrap();
    let manifest = serde_json::json!({
        "id": "com.test.gain",
        "name": "Test Gain",
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 2,
        "entrypoints": [{ "entrypoint": "extension.wasm", "target": "wasm" }],
        "capabilities": ["device", "ui.draw"],
        "devices": [{
            "type_id": "com.test.gain",
            "version": { "major": 0, "minor": 1 }
        }]
    });
    std::fs::write(
        extension.join("extension.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let wasm = wat::parse_str(module_wat(draw_json)).unwrap();
    std::fs::write(extension.join("extension.wasm"), wasm).unwrap();
    dir
}

fn write_egui_extension(frame: &EguiFrame) -> PathBuf {
    let frame_bytes = postcard::to_allocvec(frame).unwrap();
    let mut wat = module_wat(DRAW);
    let mut data = String::with_capacity(frame_bytes.len() * 4);
    for byte in &frame_bytes {
        write!(&mut data, "\\{byte:02x}").unwrap();
    }
    let packed = (8192u64 << 32) | frame_bytes.len() as u64;
    let extra = format!(
        r#"
  (data (i32.const 8192) "{data}")
  (func (export "notwig_device_supports_egui") (param i32 i32) (result i32)
    i32.const 1)
  (func (export "notwig_device_draw_egui") (param i32 i32 i32) (result i64)
    i64.const {packed})"#
    );
    let end = wat.rfind(')').unwrap();
    wat.insert_str(end, &extra);
    let dir = std::env::temp_dir().join(format!("notwig-ext-egui-test-{}", uuid::Uuid::new_v4()));
    let extension = dir.join("com.test.egui");
    std::fs::create_dir_all(&extension).unwrap();
    let manifest = serde_json::json!({
        "id": "com.test.egui",
        "name": "Test Egui",
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 2,
        "entrypoints": [{ "entrypoint": "extension.wasm", "target": "wasm" }],
        "capabilities": ["device", "ui.draw", "ui.egui"],
        "devices": [{
            "type_id": "com.test.gain",
            "version": { "major": 0, "minor": 1 }
        }]
    });
    std::fs::write(
        extension.join("extension.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        extension.join("extension.wasm"),
        wat::parse_str(&wat).unwrap(),
    )
    .unwrap();
    dir
}

fn egui_frame() -> EguiFrame {
    EguiFrame {
        primitives: vec![EguiPrimitive {
            clip_rect: [0.0, 0.0, 100.0, 100.0],
            mesh: EguiMesh {
                indices: vec![0, 1, 2],
                vertices: vec![
                    EguiVertex {
                        pos: [0.0, 0.0],
                        uv: [0.0, 0.0],
                        color: [255; 4],
                    },
                    EguiVertex {
                        pos: [10.0, 0.0],
                        uv: [1.0, 0.0],
                        color: [255; 4],
                    },
                    EguiVertex {
                        pos: [0.0, 10.0],
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
        actions: vec![PanelAction::RequestRepaint],
        cursor: None,
        wants_keyboard: false,
        wants_pointer: false,
        copy_text: None,
        repaint_after_ms: Some(16),
    }
}

fn assert_for_panel(panel: &notwig_ext_api::PanelDraw) {
    assert_eq!(panel.commands.len(), 1);
    panel.validate().unwrap();
}

fn plan_for(spec: notwig_dsp::DeviceSpec) -> (CompiledPlan, PlanDevice) {
    let config = RenderConfig {
        sample_rate: 48_000,
        max_block_size: 64,
        channels: 2,
        tempo_bpm: 120.0,
    };
    let plan = CompiledPlan {
        config,
        master: TrackId::nil(),
        order: Vec::new(),
        tracks: BTreeMap::new(),
        devices: BTreeMap::new(),
        chains: BTreeMap::new(),
        total_latency: 0,
    };
    let device = PlanDevice {
        id: DeviceId::nil(),
        type_id: "com.test.gain".into(),
        kind: DeviceKind::AudioFx,
        spec,
        param_values: BTreeMap::new(),
        bypass: false,
        child_chains: Vec::new(),
        latency_samples: 0,
        resolved: true,
    };
    (plan, device)
}

#[test]
fn loads_device_spec_and_draws_panel() {
    let dir = write_extension();
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let specs = host.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "Gain");
    assert_eq!(specs[0].version, SchemaVersion::V0);
    assert!(host.infos()[0].error.is_none());

    let parsed: notwig_ext_api::PanelDraw = serde_json::from_str(DRAW).expect("parse panel json");
    assert_for_panel(&parsed);
    let mut panel = host.panel_host();
    let context = DeviceDrawContext {
        device_id: DeviceId::nil().to_string(),
        type_id: "com.test.gain".into(),
        name: "Gain".into(),
        sample_rate: 48_000,
        width: 100.0,
        height: 100.0,
        selected: false,
        bypassed: false,
        params: BTreeMap::new(),
        theme: ThemeSnapshot::default(),
        input: notwig_ext_api::UiInput::default(),
        egui: notwig_ext_api::EguiInput::default(),
        meter: notwig_ext_api::DeviceMeterSnapshot::default(),
    };
    let draw = panel
        .draw_device_panel("com.test.gain", &context)
        .expect("panel draw");
    let notwig_ext_api::DevicePanelFrame::Commands(draw) = draw else {
        panic!("expected command panel");
    };
    assert_eq!(draw.commands.len(), 1);
    assert!(matches!(draw.commands[0], PanelCommand::Draw(_)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn extension_panel_actions_survive_host_draw() {
    let dir = write_extension_with(DRAW_WITH_ACTIONS);
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let mut panel = host.panel_host();
    let context = DeviceDrawContext {
        device_id: DeviceId::nil().to_string(),
        type_id: "com.test.gain".into(),
        name: "Gain".into(),
        sample_rate: 48_000,
        width: 100.0,
        height: 100.0,
        selected: false,
        bypassed: false,
        params: BTreeMap::new(),
        theme: ThemeSnapshot::default(),
        input: notwig_ext_api::UiInput::default(),
        egui: notwig_ext_api::EguiInput::default(),
        meter: notwig_ext_api::DeviceMeterSnapshot::default(),
    };
    let draw = panel
        .draw_device_panel("com.test.gain", &context)
        .expect("panel draw");
    let notwig_ext_api::DevicePanelFrame::Commands(draw) = draw else {
        panic!("expected command panel");
    };
    assert_eq!(draw.actions.len(), 1);
    assert!(matches!(
        draw.actions[0],
        notwig_ext_api::PanelAction::RequestRepaint
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn egui_extension_frame_crosses_the_wasm_boundary() {
    let dir = write_egui_extension(&egui_frame());
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let mut panel = host.panel_host();
    let context = DeviceDrawContext {
        device_id: DeviceId::nil().to_string(),
        type_id: "com.test.gain".into(),
        name: "Gain".into(),
        sample_rate: 48_000,
        width: 100.0,
        height: 100.0,
        selected: false,
        bypassed: false,
        params: BTreeMap::new(),
        theme: ThemeSnapshot::default(),
        input: notwig_ext_api::UiInput::default(),
        egui: notwig_ext_api::EguiInput::default(),
        meter: notwig_ext_api::DeviceMeterSnapshot::default(),
    };
    let frame = panel
        .draw_device_panel("com.test.gain", &context)
        .expect("panel draw");
    let DevicePanelFrame::Egui(frame) = frame else {
        panic!("expected egui panel");
    };
    assert_eq!(frame.primitives.len(), 1);
    assert_eq!(frame.textures.len(), 1);
    assert!(matches!(frame.actions[0], PanelAction::RequestRepaint));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wasm_device_processes_audio() {
    let dir = write_extension();
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let spec = host.specs().into_iter().next().unwrap();
    let (plan, device) = plan_for(spec);
    let factory = host.factory();
    let mut processor = factory.create(&plan, &device).expect("create processor");
    let config = RenderConfig {
        sample_rate: 48_000,
        max_block_size: 64,
        channels: 2,
        tempo_bpm: 120.0,
    };
    processor.prepare(config).unwrap();
    let transport = TransportInfo::default();
    let context = ProcessContext {
        config,
        sample_position: 0,
        playing: true,
        tempo_bpm: 120.0,
        transport: &transport,
    };
    let mut audio = vec![1.0_f32; 128];
    let status = processor.process(&context, &mut audio, &[]);
    assert!(!status.overloaded);
    assert!(audio.iter().all(|sample| (*sample - 0.5).abs() < 1e-6));
    assert_eq!(processor.latency().samples, 7);
    let _ = std::fs::remove_dir_all(dir);
}
