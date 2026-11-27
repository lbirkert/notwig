use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use notwig_ext::ExtensionHost;
use notwig_ext_api::{
    DeviceRegistrationDescriptor, ExtensionPointDescriptor, ExtensionRegistrations,
};

fn registration_module() -> String {
    let registrations = ExtensionRegistrations {
        version: notwig_ext_api::REGISTRATION_MANIFEST_VERSION,
        points: vec![ExtensionPointDescriptor {
            id: "com.test.runtime.point".into(),
        }],
        devices: vec![DeviceRegistrationDescriptor {
            point: "notwig.devices".into(),
            type_id: "com.test.runtime.device".into(),
        }],
        commands: vec![notwig_ext_api::CommandDescriptor {
            id: "com.test.runtime.command".into(),
            title: "Runtime Command".into(),
            category: "Test".into(),
            description: "Registered at runtime.".into(),
            when: Some("focus == timeline".into()),
            default_bindings: vec!["Ctrl+Alt+R".into()],
            extension_id: None,
        }],
        audio_sub_editors: vec![notwig_ext_api::AudioSubEditorDescriptor {
            id: "com.test.runtime.editor".into(),
            title: "Runtime Editor".into(),
            category: "Test".into(),
            order: 12,
            extension_id: None,
        }],
    };
    let bytes = serde_json::to_vec(&registrations).unwrap();
    let mut escaped = String::with_capacity(bytes.len() * 4);
    for byte in &bytes {
        write!(&mut escaped, "\\{byte:02x}").unwrap();
    }
    let offset = 1024_u64;
    let packed = (offset << 32) | u64::try_from(bytes.len()).unwrap();
    let spec = r#"{"type_id":"com.test.runtime.device","version":{"major":0,"minor":0},"kind":"AudioFx","ports":[],"params":[],"latency_samples":0,"name":"Runtime Device","vendor":"Test"}"#;
    let mut escaped_spec = String::with_capacity(spec.len() * 4);
    for byte in spec.as_bytes() {
        write!(&mut escaped_spec, "\\{byte:02x}").unwrap();
    }
    let spec_offset = 2048_u64;
    let spec_packed = (spec_offset << 32) | u64::try_from(spec.len()).unwrap();
    format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const {offset}) "{escaped}")
  (data (i32.const {spec_offset}) "{escaped_spec}")
  (func (export "notwig_abi_version") (result i32) i32.const 8)
  (func (export "notwig_alloc") (param i32) (result i32) i32.const 4096)
  (func (export "notwig_free") (param i32 i32))
  (func (export "notwig_ext_registrations") (result i64) i64.const {packed})
  (func (export "notwig_device_describe") (param i32 i32) (result i64) i64.const {spec_packed})
  (func (export "notwig_device_create") (param i32 i32) (result i32) i32.const -1)
  (func (export "notwig_device_destroy") (param i32))
  (func (export "notwig_device_prepare") (param i32 i32 i32 i32) (result i32) i32.const -1)
  (func (export "notwig_device_set_param") (param i32 i32 i32 i32 f64) (result i32) i32.const -1)
  (func (export "notwig_device_process") (param i32 i32 i32 i32 i32 i64 i32) (result i32) i32.const -1)
  (func (export "notwig_device_latency") (param i32) (result i32) i32.const 0)
  (func (export "notwig_device_draw") (param i32 i32 i32) (result i64) i64.const 0)
)"#
    )
}

fn write_extension() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-ext-reg-{}", uuid::Uuid::new_v4()));
    let extension = dir.join("com.test.runtime");
    std::fs::create_dir_all(&extension).unwrap();
    let manifest = serde_json::json!({
        "manifest_version": 2,
        "id": "com.test.runtime",
        "name": "Runtime Registrations",
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 8,
        "entrypoints": [{ "entrypoint": "extension.wasm", "target": "wasm" }],
        "capabilities": ["command", "audio_editor"],
        "devices": [],
        "contributes": [
            {
                "id": "com.test.runtime.command",
                "point": "notwig.command",
                "contract": "notwig.command@1",
                "title": "Manifest Command",
                "category": "Test"
            },
            {
                "id": "com.test.runtime.editor",
                "point": "notwig.audio_editor.sub_editor",
                "contract": "notwig.audio_editor.sub_editor@1",
                "title": "Manifest Editor",
                "category": "Test",
                "order": 1
            }
        ]
    });
    std::fs::write(
        extension.join("extension.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        extension.join("extension.wasm"),
        wat::parse_str(registration_module()).unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn runtime_registrations_are_used_without_manifest_contributions() {
    let root = write_extension();
    let host = ExtensionHost::load(&root, &BTreeMap::new()).expect("load host");

    assert!(
        host.extension_points()
            .iter()
            .any(|point| point.id == "com.test.runtime.point")
    );
    assert!(
        host.extension_points()
            .iter()
            .any(|point| point.id == "notwig.devices")
    );

    let specs = host.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].type_id, "com.test.runtime.device");

    let commands = host.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id, "com.test.runtime.command");
    assert_eq!(commands[0].title, "Runtime Command");
    assert_eq!(commands[0].when.as_deref(), Some("focus == timeline"));
    assert_eq!(
        commands[0].extension_id.as_deref(),
        Some("com.test.runtime")
    );

    let editors = host.audio_sub_editors();
    assert_eq!(editors.len(), 1);
    assert_eq!(editors[0].id, "com.test.runtime.editor");
    assert_eq!(editors[0].order, 12);
    assert_eq!(editors[0].extension_id.as_deref(), Some("com.test.runtime"));

    let _ = std::fs::remove_dir_all(root);
}
