# Testing and Distribution

## Test at the lowest layer

Most extension behavior can be tested without the wasm host:

- `spec()` is a serialization test: assert the type ID, ports, parameters,
  ranges, defaults, and UI hint.
- `process()` is a DSP test: feed synthetic buffers and assert limiting, gain,
  filtering, latency, and silence.
- `draw()` is a headless UI test: construct a `UiInput`, a `UiMemory`, a
  `RecordUi`, and a `UiContext`, call `draw`, then inspect the returned
  `PanelDraw`.

The `notwig-ext-devices` limiter is the reference project:

- `crates/notwig-ext-api/tests/ui_context.rs` covers click, drag, clipping, and
  action output through `RecordUi`.
- `crates/notwig-ext-api/tests/draw_validation.rs` covers command and geometry
  limits.
- `crates/notwig-ext-devices/tests/limiter.rs` exercises the device spec, panel
  output, threshold limiting, release recovery, and parameter rejection.
- `crates/notwig-ext/tests/limiter_extension.rs` loads the built limiter through
  the real wasm host when the module is present.
- `crates/notwig-ui/tests/extension_panel.rs` covers host-side panel replay and
  invalid-command fallback.

A headless draw test looks like this:

```rust
let input = UiInput::default();
let mut memory = UiMemory::default();
let mut ui_backend = RecordUi::new(&input, &mut memory);
let mut ui = UiContext::new(&mut ui_backend, Style::default(), target_rect);

device.draw(&mut ui);
let panel = ui_backend.into_panel();
panel.validate().unwrap();
assert!(panel.commands.iter().any(|command| matches!(
    command,
    PanelCommand::Draw(_)
)));
```

Drive clicks and drags by filling `UiInput` with pointer positions, button
state, modifiers, and frame time. The same input produces deterministic output.

## Test the full host path

Build the module for wasm and load it through `notwig-ext`:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p notwig-ext-devices --target wasm32-unknown-unknown --release
cargo test -p notwig-ext --test limiter_extension
cargo test -p notwig-ui --test extension_panel
```

Inline WAT modules in `notwig-ext/tests/abi.rs` cover the ABI without a wasm
toolchain. Use them as a model when testing error paths, draw-only silence, and
ABI version mismatches.

## Package the extension

The repository script builds the stock extension and writes a ZIP:

```sh
./scripts/package-extension.sh
```

It reads `crates/notwig-ext-devices/extension.json`, builds the crate for
`wasm32-unknown-unknown`, and writes
`docs/extensions/notwig.ext.devices.zip`. The ZIP contains:

```text
extension.json
notwig_ext_devices.wasm
icon.png
```

For your own extension, copy the script and replace the crate name, manifest
path, module name, and output path. Keep the ZIP root layout valid.

## Publish an index

The extension explorer lists available packages from JSON index sources. The
repository builds that index from a list of local paths or URLs:

```sh
EXTENSIONS_LIST=docs/extensions.txt ./scripts/build-extensions.sh
```

`docs/extensions.txt` contains one ZIP path or URL per line. Blank lines and
lines starting with `#` are ignored. The script:

- downloads or reads each ZIP;
- finds the single `extension.json`;
- validates `id`, `name`, `version`, and `api_version`;
- collects device names and type IDs;
- rewrites local paths to URLs relative to `docs/`;
- writes `docs/extensions.json` as a sorted JSON array.

An index entry looks like:

```json
{
  "id": "com.example.gain",
  "name": "Example Gain",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 2,
  "entrypoint": "example_gain.wasm",
  "capabilities": ["device", "ui.draw"],
  "devices": [
    { "name": "Example Gain", "type_id": "com.example.gain.fx" }
  ],
  "description": "A minimal gain device.",
  "image": "icon.png",
  "url": "extensions/example-gain.zip"
}
```

Host the index and ZIPs over HTTPS. Add the index URL in **Settings >
Extensions > Manage Indices**. Users can then install the extension from the
**Available** list.

## Release checklist

1. Update the package `version`.
2. Build the wasm module in release mode.
3. Run the SDK, device, host, and UI tests.
4. Package the ZIP and verify that it contains one valid `extension.json`.
5. Install the ZIP through the extension explorer and insert the device.
6. Save, reopen, and render a project that uses the device.
7. Publish the ZIP and update the index.
