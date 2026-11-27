# Quickstart

This page builds a minimal gain extension, packages it, and loads it in Notwig.

The quickest path is the CLI:

```sh
cargo install --path crates/notwig-ext/cargo-notwig-ext
cargo notwig-ext new my-gain
cd my-gain
cargo notwig-ext build --entrypoint wasm
cargo notwig-ext build --package wasm
```

`new` generates a working gain device, `Cargo.toml`, `extension.json`, and a
default `icon.png`. The rest of this page shows the generated files and how to
build them manually.

## 1. Create the crate

Create a library crate outside the Notwig workspace:

```text
my-gain/
  Cargo.toml
  extension.json
  src/lib.rs
```

`Cargo.toml`:

```toml
[package]
name = "my-gain"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
notwig-core = { path = "/path/to/notwig/crates/notwig-core", default-features = false }
notwig-ui = { path = "/path/to/notwig/crates/notwig-ui", default-features = false }
notwig-ext-api = { path = "/path/to/notwig/crates/notwig-ext/notwig-ext-api" }
uuid = { version = "1", default-features = false, features = ["serde", "std"] }
```

Use `default-features = false` for `notwig-core` and `notwig-ui`. That keeps
UUID randomness, the app shell, and its file I/O out of the wasm module. The
`notwig-ui` components still provide the shared knobs, meters, labels, and
palette. Param IDs are constructed from fixed UUID integers instead of random
values.

## 2. Implement the device

`src/lib.rs`:

```rust
use notwig_core::dsp::{
    DeviceSpec, DeviceUiSpec, ParamSpec, Port, PortDir, SignalType, Unit,
};
use notwig_core::{DeviceKind, ParamId, SchemaVersion};
use notwig_ext_api::{EP_NOTWIG_DEVICES, DeviceExtension, DeviceMeterSnapshot, ExtensionBuilder, egui, panel_frame, notwig_ext};
use notwig_ui::{knob, section};
use uuid::Uuid;

pub const TYPE_ID: &str = "com.example.gain";

const GAIN_UUID: u128 = 0x6761_696e_0000_0000_0000_0000_0000_0001;
const DEFAULT_DB: f64 = 0.0;

fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(GAIN_UUID))
}

fn gain_norm(db: f64) -> f32 {
    ((db + 24.0) / 48.0) as f32
}

#[derive(Default)]
struct GainDevice {
    gain_db: f64,
    gain: f32,
}

impl GainDevice {
    fn update_gain(&mut self) {
        self.gain = 10.0_f64.powf(self.gain_db / 20.0) as f32;
    }
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<GainDevice>(EP_NOTWIG_DEVICES);
}

impl DeviceExtension for GainDevice {
    fn new() -> Self {
        let mut device = Self {
            gain_db: DEFAULT_DB,
            gain: 1.0,
        };
        device.update_gain();
        device
    }

    fn spec() -> DeviceSpec {
        DeviceSpec {
            type_id: TYPE_ID.into(),
            version: SchemaVersion::V0,
            kind: DeviceKind::AudioFx,
            ports: vec![
                Port {
                    id: "in".into(),
                    dir: PortDir::In,
                    signal: SignalType::Audio(2),
                    name: "Input".into(),
                    optional: false,
                },
                Port {
                    id: "out".into(),
                    dir: PortDir::Out,
                    signal: SignalType::Audio(2),
                    name: "Output".into(),
                    optional: false,
                },
            ],
            params: vec![ParamSpec::continuous(
                gain_id(),
                "Gain",
                Unit::Decibels,
                -24.0,
                24.0,
                DEFAULT_DB,
            )],
            latency_samples: 0,
            name: "My Gain".into(),
            vendor: "Example".into(),
            ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(180.0)),
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
        section(ui, "My Gain");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(78.0, 58.0), egui::Sense::hover());
        if let Some(norm) = knob(
            ui,
            ui.make_persistent_id("gain"),
            rect,
            gain_norm(self.gain_db),
            gain_norm(DEFAULT_DB),
            true,
            "Gain",
        ) {
            self.gain_db = -24.0 + f64::from(norm) * 48.0;
            self.update_gain();
            panel_frame::set_param_normalized(ui, gain_id(), norm);
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
        if param_id != gain_id() {
            return false;
        }
        self.gain_db = value.clamp(-24.0, 24.0);
        self.update_gain();
        true
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            *sample *= self.gain;
        }
    }
}
```

`#[notwig_ext(load)]` generates the ABI exports around the entrypoint: WASM
exports when compiling for `wasm32-unknown-unknown` and a native shared-library
ABI on desktop targets. The device implements only the Rust surface.

An extension may also define optional `#[notwig_ext(preload)]` and
`#[notwig_ext(unload)]` functions. Preload declares extension points before any
load-phase registration; unload releases extension-owned resources. The `load`
phase is required and remains the place for devices, commands, and sub-editors.

Keep `lib.rs` as wiring only: module declarations, re-exports, imports needed by
the lifecycle functions, and the lifecycle entrypoints. Put device, command,
panel, and helper implementations in other modules. `load`, `unload`, and any
explicit reload hook must be the final items in `lib.rs`.

## 3. Add the manifest

`extension.json`:

```json
{
  "id": "com.example.gain",
  "name": "My Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 8,
  "capabilities": ["device", "ui.panel"],
  "image": "icon.png",
  "devices": [
    {
      "name": "My Gain",
      "type_id": "com.example.gain"
    }
  ]
}
```

The source manifest omits `entrypoints`; the CLI adds the selected target
entrypoint when packaging. Add extra runtime assets with `includes`:

```json
"includes": [
  { "path": "assets/impulse.wav" },
  { "path": "assets/desktop-only.bin", "target": "macos-aarch64" }
]
```

`extension.json` and `icon.png` are included automatically. The `includes` field
is removed from the packaged manifest.

The manifest `id`, the device `type_id`, and the renderer string in
`DeviceUiSpec::custom` should agree. See
[Package and Manifest](02_package_and_manifest.md) for every field.

## 4. Build

For a portable WASM build:

```sh
rustup target add wasm32-unknown-unknown
cargo notwig-ext build --entrypoint wasm
cargo notwig-ext build --package wasm
```

`--entrypoint` runs `cargo build --release` and leaves the binary in Cargo's
`target/<rust-triple>/release/` directory. `--package` adds the `entrypoints`
field and bundles the binaries that are present into a ZIP:

```text
dist/com.example.gain-wasm.zip
```

For a native build on macOS Apple Silicon:

```sh
cargo notwig-ext build --entrypoint macos-aarch64
cargo notwig-ext build --package macos-aarch64
```

To bundle several platforms, run `--entrypoint` on each target VM, copy the
`target/<rust-triple>/release/` artifacts into one target directory, then run
`cargo notwig-ext build --package`. Use `--out-dir` and `--name` to choose the
output location or filename:

```sh
cargo notwig-ext build --package macos-aarch64 --out-dir docs/extensions --name my-gain-macos
```

Native libraries are not sandboxed. See
[Package and Manifest](02_package_and_manifest.md) for target names and the
trust implications.

## 5. Install and load

Use **Settings > Extensions > Add .ZIP** and select the generated ZIP.
Installing reloads the extension host automatically.

You can also copy an installed package directory into the user extension
directory, but the ZIP path is preferred because it includes the transformed
manifest and entrypoint. Restart Notwig after a manual copy. The device then
appears under **EXTENSIONS** in the Add Device browser.

## 6. Verify

1. Select an audio track and open the device editor.
2. Click a **+** insertion point.
3. Search for **My Gain** and press **Enter**.
4. Drag the knob. The parameter should move and the sound should change.
5. Save and reopen the project. The parameter value should be restored.

If the device does not appear, check the manifest `api_version`, the build
target, the device `type_id`, and the extension error shown in
**Settings > Extensions**.

Next: [Package and Manifest](02_package_and_manifest.md).
