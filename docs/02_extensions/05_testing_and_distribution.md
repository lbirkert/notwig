# Testing and Distribution

## Test at the lowest layer

Most extension behavior can be tested without the wasm host:

- `spec()` is a serialization test: assert the type ID, ports, parameters,
  ranges, defaults, and UI hint.
- `process()` is a DSP test: feed synthetic buffers and assert limiting, gain,
  filtering, latency, and silence.
- `draw()` is a headless UI test: run the guest panel frame with
  `panel_frame::run_frame`, then validate the returned `PanelFrame`.

The `notwig-ext-devices` limiter and gate are the reference projects:

- `crates/notwig-ext/notwig-ext-api/tests/panel_bridge.rs` covers guest frame generation and
  action output.
- `crates/notwig-ext/notwig-ext-api/tests/panel_wire.rs` covers frame validation and limits.
- `crates/notwig-ext/notwig-ext-api/tests/note_events.rs` covers the note-event wire format.
- `crates/notwig-ext/notwig-ext-devices/tests/limiter.rs` exercises the limiter spec, egui
  frame output, threshold limiting, release recovery, and parameter rejection.
- `crates/notwig-ext/notwig-ext-devices/tests/gate.rs` exercises the gate spec, panel frame
  output, open/close behavior, and parameter rejection.
- `crates/notwig-ext/notwig-ext-devices/tests/reverb.rs` exercises the container
  spec and wet parameter, pre-delay, Room/Hall/Plate tails, and parameter
  rejection.
- `crates/notwig-ext/notwig-ext-devices/tests/{compressor,clipper,delay,chorus,transient}.rs`
  cover the remaining bundled effects.
- `crates/notwig-core/tests/dsp_container.rs` exercises host-owned container
  routing, wet blending, nested parameters, and latency.
- `crates/notwig-ext/notwig-ext-devices/tests/synth.rs` exercises the synth spec, egui
  frame output, oscillator modes, note on/off, ADSR release, and parameter
  rejection.
- `crates/notwig-ext/notwig-ext-devices/tests/gain_device.rs` exercises the gain device.
- `crates/notwig-ext/notwig-ext/tests/limiter_extension.rs` loads the built extension
  through the real wasm host when the module is present, including synth note
  playback.
- `crates/notwig-ui/tests/extension_panel.rs` covers host-side compositing,
  meter delivery, and invalid-frame handling.

A headless draw test looks like this:

```rust
let context = notwig_ext_api::egui::Context::default();
let input = notwig_ext_api::PanelInput {
    width: 260.0,
    height: 200.0,
    pixels_per_point: 1.0,
    time: Some(0.0),
    focused: true,
    max_texture_side: 4096,
    modifiers: notwig_ext_api::PanelModifiers::default(),
    events: Vec::new(),
};
let frame = notwig_ext_api::panel_frame::run_frame(
    &context,
    input,
    &notwig_ext_api::ThemeSnapshot::default(),
    |ui| device.draw(ui, notwig_ext_api::DeviceMeterSnapshot::default()),
);
frame.validate().unwrap();
assert_ne!(frame.primitives.len(), 0);
```

Drive clicks and drags by filling the `PanelInput` event list with pointer and
keyboard events. The same input produces deterministic output.

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
`crates/notwig-ext/notwig-ext/tests/abi.rs` cover the WASM ABI without a wasm toolchain. Use them
as a model when testing error paths and ABI version mismatches.

## Package the extension

The CLI separates building one target binary from packaging the binaries
together:

```sh
# Run on each target machine or VM.
cargo notwig-ext build --entrypoint wasm
cargo notwig-ext build --entrypoint macos-aarch64

# Copy target/<rust-triple>/release/ artifacts into one target directory, then:
cargo notwig-ext build --package --out-dir docs/extensions
```

`--package` includes every target whose artifact is present. Pass target names
to limit the package, or use `--name` to choose the ZIP name. One package can
contain several entrypoints; the host picks the first supported target. Missing
Rust targets are installed with `rustup target add` for `--entrypoint` builds by
default.

The Pages workflow uses the same commands for the stock extension. It installs
the CLI with `cargo install --path crates/notwig-ext/cargo-notwig-ext --locked`,
builds one target entrypoint on each runner, and packages that artifact into a
ZIP. Add new targets to the CLI target table and to the CI matrix when you are
ready to publish them.

## Publish an index

The extension explorer lists available packages from JSON index sources. The
repository builds that index from `docs/extensions.csv`:

```sh
EXTENSIONS_LIST=docs/extensions.csv ./scripts/build-extensions.sh
```

The CSV has a `source,hash` header. Each row is one target ZIP, local or remote:

```csv
source,hash
# macOS extension builds are temporarily disabled in Pages CI.
# docs/extensions/ext-devices-macos-arm64.zip,
docs/extensions/ext-devices-wasm.zip,
# docs/extensions/ext-example-macos-arm64.zip,
docs/extensions/ext-example-wasm.zip,
```

The Pages workflow publishes both the stock device pack and the command API
example for WASM. macOS extension builds are temporarily disabled to keep CI
runtime down. Install `Notwig Extension Examples` from the index to test the
`Humanize Notes` command.

Hashes are SHA-256 lowercase hex. Leave a hash blank to compute it while building
the index, or fill it in to pin an existing file; a non-empty hash is verified
against the ZIP. Blank lines and lines starting with `#` are ignored. The script:

- reads or downloads each ZIP and verifies or computes its hash;
- finds the single `extension.json`;
- validates `id`, `name`, `version`, and `api_version`;
- reads the manifest `entrypoints` and preserves their target order;
- copies the package icon to `docs/extensions/<id>.png` and points the index
  entry at `extensions/<id>.png`;
- collects device names and type IDs;
- records each package's compressed size in bytes;
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
  "api_version": 7,
  "capabilities": ["device", "ui.panel"],
  "devices": [
    { "name": "Example Gain", "type_id": "com.example.gain.fx" }
  ],
  "description": "A minimal gain device.",
  "image": "extensions/com.example.gain.png",
  "urls": [
    {
      "url": "extensions/example-gain-macos-arm64.zip",
      "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "size": 1048576,
      "targets": ["macos-aarch64"]
    },
    {
      "url": "extensions/example-gain-wasm.zip",
      "hash": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      "size": 786432,
      "targets": ["wasm"]
    }
  ]
}
```

The client chooses the first `urls` entry with a supported target, downloads it,
verifies the SHA-256 hash, and only then installs the ZIP. The explorer shows the
selected package's `size` when present; `0` or a missing field means unknown.
This lets one extension ship slim per-target downloads or a single fat ZIP whose
`targets` array lists several targets.

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
   `extension.json` with the right target and `api_version: 7`.
5. Install the ZIPs through the extension explorer and insert the device.
6. Save, reopen, and render a project that uses the device.
7. Publish the ZIPs, fill or verify the hashes in `docs/extensions.csv`, and
   rebuild the index.
