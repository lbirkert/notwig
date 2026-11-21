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

The `notwig-ext-devices` limiter and gate are the reference projects:

- `crates/notwig-ext-api/tests/ui_context.rs` covers click, drag, clipping,
  stereo-meter painting, dropdown selection, and action output through
  `RecordUi`.
- `crates/notwig-ext-api/tests/note_events.rs` covers the note-event wire format.
- `crates/notwig-ext-api/tests/draw_validation.rs` covers command and geometry
  limits.
- `crates/notwig-ext-devices/tests/limiter.rs` exercises the limiter spec, panel
  output, threshold limiting, release recovery, and parameter rejection.
- `crates/notwig-ext-devices/tests/gate.rs` exercises the gate spec, panel
  output, open/close behavior, and parameter rejection.
- `crates/notwig-ext-devices/tests/synth.rs` exercises the synth spec, panel,
  oscillator modes, note on/off, ADSR release, and parameter rejection.
- `crates/notwig-ext/tests/limiter_extension.rs` loads the built extension
  through the real wasm host when the module is present, including synth note
  playback.
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

Build the target binaries and load them through `notwig-ext`:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p notwig-ext-devices --target wasm32-unknown-unknown --release
cargo build -p notwig-ext-devices --target aarch64-apple-darwin --release
cargo test -p notwig-ext --test limiter_extension
cargo test -p notwig-ui --test extension_panel
```

`limiter_extension.rs` tests both the WASM host and the native host when the
corresponding binaries are present. Inline WAT modules in
`notwig-ext/tests/abi.rs` cover the WASM ABI without a wasm toolchain. Use them
as a model when testing error paths and ABI version mismatches.

## Package the extension

The repository script builds one target and writes a ZIP:

```sh
./scripts/package-extension.sh wasm
./scripts/package-extension.sh macos-aarch64
```

It reads the `crates/notwig-ext-devices/extension.json` template, builds the
crate for the requested target, substitutes `$$TARGET` and `$$ENTRYPOINT` in the
manifest, and writes `docs/extensions/ext-devices-<target>.zip`. Each ZIP
contains:

```text
extension.json
notwig_ext_devices.wasm   # or .dylib/.so/.dll for a native target
icon.png
```

With no argument the script builds WASM, plus the macOS Apple Silicon build on an
Apple Silicon host. The target-to-cargo mapping is a `case` statement, so adding
Windows or Linux builds is a small local change; add the same target to the CI
matrix when you are ready to publish it.

For your own extension, copy the script and replace the crate name, manifest
path, output paths, and manifest placeholders. Keep the ZIP root layout valid.

## Publish an index

The extension explorer lists available packages from JSON index sources. The
repository builds that index from `docs/extensions.csv`:

```sh
EXTENSIONS_LIST=docs/extensions.csv ./scripts/build-extensions.sh
```

The CSV has a `source,hash` header. Each row is one target ZIP, local or remote:

```csv
source,hash
docs/extensions/ext-devices-macos-arm64.zip,
docs/extensions/ext-devices-wasm.zip,
```

Hashes are SHA-256 lowercase hex. Leave a hash blank to compute it while building
the index, or fill it in to pin an existing file; a non-empty hash is verified
against the ZIP. Blank lines and lines starting with `#` are ignored. The script:

- reads or downloads each ZIP and verifies or computes its hash;
- finds the single `extension.json`;
- validates `id`, `name`, `version`, and `api_version`;
- reads the manifest `entrypoints` and preserves their target order;
- collects device names and type IDs;
- merges rows with the same extension `id` into one entry with several
  `urls`;
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
  "capabilities": ["device", "ui.draw"],
  "devices": [
    { "name": "Example Gain", "type_id": "com.example.gain.fx" }
  ],
  "description": "A minimal gain device.",
  "image": "icon.png",
  "urls": [
    {
      "url": "extensions/example-gain-macos-arm64.zip",
      "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "targets": ["macos-aarch64"]
    },
    {
      "url": "extensions/example-gain-wasm.zip",
      "hash": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      "targets": ["wasm"]
    }
  ]
}
```

The client chooses the first `urls` entry with a supported target, downloads it,
verifies the SHA-256 hash, and only then installs the ZIP. This lets one
extension ship slim per-target downloads or a single fat ZIP whose `targets`
array lists several targets.

Host the index and ZIPs over HTTPS. Add the index URL in **Settings >
Extensions > Manage Indices**. Users can then install the extension from the
**Available** list.

The native and browser builds use the same index format. The browser fetches
the index with `fetch`, installs a selected ZIP by extracting it into
`notwig-extensions/<id>/` in OPFS, and rediscovers those folders on startup.
Extension index settings are stored in browser local storage.

## Release checklist

1. Update the package `version`.
2. Build every published target in release mode.
3. Run the SDK, device, host, and UI tests.
4. Package each target ZIP and verify that it contains one valid
   `extension.json` with the right target.
5. Install the ZIPs through the extension explorer and insert the device.
6. Save, reopen, and render a project that uses the device.
7. Publish the ZIPs, fill or verify the hashes in `docs/extensions.csv`, and
   rebuild the index.
