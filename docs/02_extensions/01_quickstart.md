# Quickstart

This page builds a minimal gain extension, packages it, and loads it in Notwig.

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
notwig-ext-api = { path = "/path/to/notwig/crates/notwig-ext-api" }
uuid = "1"
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
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, Extension, egui, egui_panel, notwig_extension};
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

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<GainDevice>();
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

    fn draw_egui(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
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
            egui_panel::set_param_normalized(ui, gain_id(), norm);
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

`#[notwig_extension]` generates the ABI exports around the entrypoint: WASM
exports when compiling for `wasm32-unknown-unknown` and a native shared-library
ABI on desktop targets. The device implements only the Rust surface.

## 3. Add the manifest

`extension.json`:

```json
{
  "id": "com.example.gain",
  "name": "My Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 3,
  "entrypoints": [
    {
      "entrypoint": "my_gain.wasm",
      "target": "wasm"
    }
  ],
  "capabilities": ["device", "ui.egui"],
  "devices": [
    {
      "name": "My Gain",
      "type_id": "com.example.gain"
    }
  ]
}
```

The manifest `id`, the device `type_id`, and the renderer string in
`DeviceUiSpec::custom` should agree. See
[Package and Manifest](02_package_and_manifest.md) for every field.

## 4. Build

For a portable WASM build:

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

The module is written to:

```text
target/wasm32-unknown-unknown/release/my_gain.wasm
```

Cargo converts dashes in the crate name to underscores in the output filename.
Copy or rename it to match the manifest `entrypoint` field.

For a native build on macOS Apple Silicon:

```sh
cargo build --target aarch64-apple-darwin --release
```

The shared library is written to:

```text
target/aarch64-apple-darwin/release/libmy_gain.dylib
```

Add a native entrypoint before the WASM entry so desktop users load the shared
library and the browser still has a portable fallback:

```json
"entrypoints": [
  { "entrypoint": "my_gain.dylib", "target": "macos-aarch64" },
  { "entrypoint": "my_gain.wasm", "target": "wasm" }
]
```

Native libraries are not sandboxed. See
[Package and Manifest](02_package_and_manifest.md) for target names and the
trust implications.

## 5. Install and load

The easiest path is to package the manifest and module into a ZIP whose root
contains `extension.json`, then use **Settings > Extensions > Add .ZIP**.
Installing reloads the extension host automatically.

You can also copy the directory into the user extension directory:

```text
~/Library/Application Support/notwig/extensions/com.example.gain/
  extension.json
  my_gain.dylib       # native target, optional
  my_gain.wasm
  icon.png            # optional
```

Restart Notwig after a manual copy. The device then appears under
**EXTENSIONS** in the Add Device browser.

## 6. Verify

1. Select an audio track and open the device editor.
2. Click a **+** insertion point.
3. Search for **My Gain** and press **Enter**.
4. Drag the knob. The parameter should move and the sound should change.
5. Save and reopen the project. The parameter value should be restored.

If the device does not appear, check the manifest `api_version`, the
`entrypoints` filenames and targets, the device `type_id`, and the extension
error shown in **Settings > Extensions**.

Next: [Package and Manifest](02_package_and_manifest.md).
