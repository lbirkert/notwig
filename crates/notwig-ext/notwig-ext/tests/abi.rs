use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use notwig_core::dsp::{CompiledPlan, PlanDevice, ProcessContext, RenderConfig, TransportInfo};
use notwig_core::{DeviceId, DeviceKind, SchemaVersion, TrackId};
use notwig_ext::ExtensionHost;
use notwig_ext_api::{
    DeviceDrawContext, PanelAction, PanelFrame, PanelImage, PanelInput, PanelMesh, PanelPrimitive,
    PanelTextureDelta, PanelTextureOptions, PanelVertex, ThemeSnapshot,
};

const DESCRIBE: &str = r#"{"type_id":"com.test.gain","version":{"major":0,"minor":1},"kind":"AudioFx","ports":[],"params":[],"latency_samples":0,"name":"Gain","vendor":"Test"}"#;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn valid_frame() -> PanelFrame {
    PanelFrame {
        primitives: vec![PanelPrimitive {
            clip_rect: [0.0, 0.0, 100.0, 100.0],
            mesh: PanelMesh {
                indices: vec![0, 1, 2],
                vertices: vec![
                    PanelVertex {
                        pos: [0.0, 0.0],
                        uv: [0.0, 0.0],
                        color: [255; 4],
                    },
                    PanelVertex {
                        pos: [10.0, 0.0],
                        uv: [1.0, 0.0],
                        color: [255; 4],
                    },
                    PanelVertex {
                        pos: [0.0, 10.0],
                        uv: [0.0, 1.0],
                        color: [255; 4],
                    },
                ],
                texture_id: 0,
            },
        }],
        textures: vec![PanelTextureDelta {
            id: 0,
            pos: None,
            options: PanelTextureOptions {
                linear: true,
                repeat: false,
            },
            image: PanelImage {
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

fn module_wat() -> String {
    let describe = escape(DESCRIBE);
    let frame_bytes = postcard::to_allocvec(&valid_frame()).unwrap();
    let mut frame = String::with_capacity(frame_bytes.len() * 4);
    for byte in &frame_bytes {
        write!(&mut frame, "\\{byte:02x}").unwrap();
    }
    let describe_packed = DESCRIBE.len() as u64;
    let frame_packed = (8192u64 << 32) | frame_bytes.len() as u64;
    format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 0) "{describe}")
  (data (i32.const 8192) "{frame}")
  (global $next (mut i32) (i32.const 16384))
  (func (export "notwig_abi_version") (result i32) i32.const 8)
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
    i64.const {describe_packed})
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
    i64.const {frame_packed})
)"#
    )
}

fn write_extension(with_ui: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-ext-test-{}", uuid::Uuid::new_v4()));
    let extension = dir.join("com.test.gain");
    std::fs::create_dir_all(&extension).unwrap();
    let capabilities = if with_ui {
        serde_json::json!(["device", "ui.panel"])
    } else {
        serde_json::json!(["device"])
    };
    let manifest = serde_json::json!({
        "id": "com.test.gain",
        "name": "Test Gain",
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 8,
        "entrypoints": [{ "entrypoint": "extension.wasm", "target": "wasm" }],
        "capabilities": capabilities,
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
        wat::parse_str(module_wat()).unwrap(),
    )
    .unwrap();
    dir
}

fn context() -> DeviceDrawContext {
    DeviceDrawContext {
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
        input: PanelInput::default(),
        meter: notwig_ext_api::DeviceMeterSnapshot::default(),
    }
}

fn plan_for(spec: notwig_core::dsp::DeviceSpec) -> (CompiledPlan, PlanDevice) {
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
        active: true,
        child_chains: Vec::new(),
        latency_samples: 0,
        resolved: true,
    };
    (plan, device)
}

#[test]
fn loads_device_spec_and_draws_panel_frame() {
    let dir = write_extension(true);
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let specs = host.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "Gain");
    assert_eq!(specs[0].version, SchemaVersion::V0);
    assert!(host.infos()[0].error.is_none());

    let mut panel = host.panel_host();
    let frame = panel
        .draw_device_panel("com.test.gain", &context())
        .expect("panel draw");
    assert_eq!(frame.primitives.len(), 1);
    assert_eq!(frame.textures.len(), 1);
    assert!(matches!(frame.actions[0], PanelAction::RequestRepaint));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn extension_without_ui_capability_has_no_panel() {
    let dir = write_extension(false);
    let host = ExtensionHost::load(&dir, &BTreeMap::new()).unwrap();
    let mut panel = host.panel_host();
    assert!(
        panel
            .draw_device_panel("com.test.gain", &context())
            .is_none()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wasm_device_processes_audio() {
    let dir = write_extension(true);
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
