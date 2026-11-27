use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use notwig_ext::ExtensionHost;
use notwig_ext_api::{CommandContext, CommandEffect};

fn effect_data() -> (String, usize, u64) {
    let bytes = postcard::to_allocvec(&CommandEffect::None).unwrap();
    let mut escaped = String::with_capacity(bytes.len() * 4);
    for byte in &bytes {
        write!(&mut escaped, "\\{byte:02x}").unwrap();
    }
    let offset = 1024_u64;
    let packed = (offset << 32) | u64::try_from(bytes.len()).unwrap();
    (escaped, bytes.len(), packed)
}

fn module_wat() -> String {
    let (data, _len, packed) = effect_data();
    format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 1024) "{data}")
  (func (export "notwig_abi_version") (result i32) i32.const 7)
  (func (export "notwig_alloc") (param i32) (result i32) i32.const 2048)
  (func (export "notwig_free") (param i32 i32))
  (func (export "notwig_device_describe") (param i32 i32) (result i64) i64.const 0)
  (func (export "notwig_device_create") (param i32 i32) (result i32) i32.const -1)
  (func (export "notwig_device_destroy") (param i32))
  (func (export "notwig_device_prepare") (param i32 i32 i32 i32) (result i32) i32.const -1)
  (func (export "notwig_device_set_param") (param i32 i32 i32 i32 f64) (result i32) i32.const -1)
  (func (export "notwig_device_process") (param i32 i32 i32 i32 i32 i64 i32) (result i32) i32.const -1)
  (func (export "notwig_device_latency") (param i32) (result i32) i32.const 0)
  (func (export "notwig_device_draw") (param i32 i32 i32) (result i64) i64.const 0)
  (func (export "notwig_command_execute") (param i32 i32 i32 i32) (result i64)
    i64.const {packed})
)"#
    )
}

fn write_extension() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-ext-command-{}", uuid::Uuid::new_v4()));
    let extension = dir.join("com.test.commands");
    std::fs::create_dir_all(&extension).unwrap();
    let manifest = serde_json::json!({
        "manifest_version": 2,
        "id": "com.test.commands",
        "name": "Test Commands",
        "vendor": "Test",
        "version": "0.1.0",
        "api_version": 7,
        "entrypoints": [{ "entrypoint": "extension.wasm", "target": "wasm" }],
        "capabilities": ["command", "audio_editor"],
        "devices": [],
        "contributes": [{
            "id": "com.test.commands.noop",
            "point": "notwig.command",
            "contract": "notwig.command@1",
            "title": "No-op",
            "category": "Test",
            "when": "focus == midi_editor"
        }, {
            "id": "com.test.commands.pan",
            "point": "notwig.audio_editor.sub_editor",
            "contract": "notwig.audio_editor.sub_editor@1",
            "title": "Clip Pan",
            "category": "Audio",
            "order": 5
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

#[test]
fn host_resolves_and_executes_wasm_commands() {
    let root = write_extension();
    let host = ExtensionHost::load(&root, &BTreeMap::new()).expect("load host");
    let commands = host.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id, "com.test.commands.noop");
    assert_eq!(
        commands[0].extension_id.as_deref(),
        Some("com.test.commands")
    );
    let effect = host
        .execute_command("com.test.commands.noop", &CommandContext::default())
        .expect("execute command");
    assert_eq!(effect, CommandEffect::None);

    let editors = host.audio_sub_editors();
    assert_eq!(editors.len(), 1);
    assert_eq!(editors[0].id, "com.test.commands.pan");
    assert_eq!(
        editors[0].extension_id.as_deref(),
        Some("com.test.commands")
    );
    let _ = std::fs::remove_dir_all(root);
}
