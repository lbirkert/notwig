use std::collections::BTreeMap;
use std::path::Path;

use notwig_ext::ExtensionHost;
use notwig_ext_api::ExtensionStatus;

const SPEC: &str = r#"{"type_id":"com.test.gain","version":{"major":0,"minor":0},"kind":"AudioFx","ports":[],"params":[],"latency_samples":0,"name":"Gain","vendor":"Test"}"#;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn stub_module(describe: &str) -> Vec<u8> {
    let wat = format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 0) "{describe}")
  (func (export "notwig_abi_version") (result i32) i32.const 9)
  (func (export "notwig_alloc") (param i32) (result i32) i32.const 4096)
  (func (export "notwig_free") (param i32 i32))
  (func (export "notwig_device_describe") (param i32 i32) (result i64)
    i64.const {packed})
  (func (export "notwig_device_create") (param i32 i32) (result i32) i32.const -1)
  (func (export "notwig_device_destroy") (param i32))
  (func (export "notwig_device_prepare") (param i32 i32 i32 i32) (result i32) i32.const -4)
  (func (export "notwig_device_set_param") (param i32 i32 i32 i32 f64) (result i32) i32.const -4)
  (func (export "notwig_device_process") (param i32 i32 i32 i32 i32 i64 i32) (result i32) i32.const -4)
  (func (export "notwig_device_latency") (param i32) (result i32) i32.const 0)
  (func (export "notwig_device_draw") (param i32 i32 i32) (result i64) i64.const 0)
)"#,
        packed = if describe.is_empty() {
            0
        } else {
            SPEC.len() as u64
        }
    );
    wat::parse_str(wat).unwrap()
}

fn write_package(root: &Path, bad: &[u8], good: &[u8]) {
    let dir = root.join("com.test.fallback");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = serde_json::json!({
        "id": "com.test.fallback",
        "name": "Fallback",
        "version": "0.1.0",
        "api_version": 9,
        "entrypoints": [
            { "entrypoint": "bad.wasm", "target": "wasm" },
            { "entrypoint": "good.wasm", "target": "wasm" }
        ],
        "devices": [{ "type_id": "com.test.gain" }]
    });
    std::fs::write(
        dir.join("extension.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("bad.wasm"), bad).unwrap();
    std::fs::write(dir.join("good.wasm"), good).unwrap();
}

#[test]
fn falls_back_to_the_next_entrypoint_when_describe_is_empty() {
    let root = std::env::temp_dir().join(format!("notwig-fallback-{}", uuid::Uuid::new_v4()));
    write_package(&root, &stub_module(""), &stub_module(&escape(SPEC)));
    let host = ExtensionHost::load(&root, &BTreeMap::new()).unwrap();
    assert_eq!(host.specs().len(), 1);
    assert_eq!(host.specs()[0].name, "Gain");
    assert!(host.has_type("com.test.gain"));
    assert!(
        host.infos()
            .iter()
            .any(|info| info.status == ExtensionStatus::Loaded)
    );
    let _ = std::fs::remove_dir_all(root);
}
