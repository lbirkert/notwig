# notwig-ext-devices

Example extension devices for Notwig, built against the `notwig-ext-api`
contract. The same crate produces a portable WASM module and a native shared
library.

## Devices

### Limiter

`notwig.ext.devices.limiter`

A stereo-linked peak limiter with live meters and a dynamics graph:

- **Threshold**: -24 dB to 0 dB, default -6 dB.
- **Release**: 0.01 s to 0.5 s, default 0.1 s.
- 64-sample lookahead (~1.33 ms at 48 kHz) with instant gain reduction inside the lookahead window and release-smoothed recovery. Reports 64 samples of latency for delay compensation.
- A stereo output meter with peak hold.
- A 50-frame peak-history graph. The signal is drawn as a filled envelope, the
  threshold as a blue line, and local gain reduction lowers that threshold line
  at each point where the limiter is working.
- A live gain-reduction readout in the graph.

### Gate

`notwig.ext.devices.gate`

A stereo-linked noise gate with the same graph and meter treatment:

- **Threshold**: -80 dB to 0 dB, default -40 dB.
- **Attack**: 0.001 s to 0.1 s, default 0.005 s.
- **Hold**: 0 s to 0.5 s, default 0.05 s.
- **Release**: 0.01 s to 1 s, default 0.1 s.
- **Range**: -80 dB to 0 dB, default -80 dB.
- A fixed blue threshold line, a filled input envelope, and an amber gate-gain
  line showing when the gate opens and closes.
- A stereo output meter with peak hold and an OPEN/CLOSED readout.

### Synth

`notwig.ext.devices.synth`

A simple polyphonic synthesizer:

- **Oscillator**: Sine, Square, Saw, or Triangle.
- **Volume ADSR**: Attack, Decay, Sustain, and Release.
- 16 voices with pitch-matched note on/off handling and a fixed per-voice gain.
- Receives note events through the extension note-event ABI.

The panel uses the shared `dropdown` widget for the oscillator and four knobs
for the ADSR. The dropdown list is drawn last so it overlays the rest of the
panel and closes when clicking outside it.

### Egui Gain (optional)

Enabled with `--features egui`, this device demonstrates the guest-owned egui
panel path. It implements `draw_egui` with a real `egui::Slider`, sets
`SUPPORTS_EGUI = true`, and reports parameter changes through
`egui_panel::set_param_value`. The crate uses `#[notwig_extension(egui)]` only
when that feature is enabled. An egui-capable package must also declare
`"ui.egui"` in its manifest capabilities.

All command-API panels are drawn dynamically each frame with `DeviceExtension::draw` and
request repaint so they animate while visible. Meter and history data come from
the engine-measured `DeviceMeterSnapshot` exposed through
`UiContext::device_meter()`, because the panel runs in a separate extension
instance from the audio processor. The shared stereo meter painter lives in
`notwig_ext_api::ui`, so other extension devices can reuse it. The graph and
history ring are local to this crate and shared by the limiter and gate.

The devices are implemented with the SDK trait and registered from the macro
entrypoint:

```rust
use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
    extension.register_device::<GateDevice>();
    extension.register_device::<SynthDevice>();
}
```

Each device implements `DeviceExtension`, providing `spec`, `draw`, `prepare`,
`set_param`, `process`, and `latency`. Interaction state is retained by the
extension runtime and the host widget renderer; the devices keep only their own
parameter values and display state. The generated ABI exports and all
heap/store/pointer plumbing come from `notwig-ext-api`.

## Build

For WASM:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p notwig-ext-devices --target wasm32-unknown-unknown --release
```

The module is written to:

```text
target/wasm32-unknown-unknown/release/notwig_ext_devices.wasm
```

For macOS Apple Silicon:

```sh
cargo build -p notwig-ext-devices --target aarch64-apple-darwin --release
```

The shared library is written to:

```text
target/aarch64-apple-darwin/release/libnotwig_ext_devices.dylib
```

The repository packaging script substitutes the manifest placeholders and writes
target-specific ZIPs:

```sh
./scripts/package-extension.sh wasm
./scripts/package-extension.sh macos-aarch64
```

## Install

Install the generated ZIP from **Settings > Extensions > Add .ZIP**, or copy the
manifest and target binary into a directory under the user extensions directory:

```text
~/Library/Application Support/notwig/extensions/notwig.ext.devices/
  extension.json
  notwig_ext_devices.dylib   # native target, optional
  notwig_ext_devices.wasm
```

Then open **Settings > Extensions** and press **Reload extensions**. The
Limiter, Gate, and Synth appear under **Extensions** in the Add Device browser.

The manifest template uses `"api_version": 2` and `$$TARGET` / `$$ENTRYPOINT`
placeholders that the packaging script replaces for each target.

## Notes

- Heap, device-store, pointer/audio helpers, and the ABI exports are generated
  by the SDK and `#[notwig_extension]`. This crate contains only the device
  implementations and their spec/panel code.
- WASM uses the SDK's fixed bump heap so pointers stay stable for the lifetime
  of the module instance. Native builds pass process pointers directly.
- The extension crate does not enable `unsafe`; generated code keeps its
  `no_mangle` attributes inside an SDK-generated module.
- The synth receives note events through the extension note-event ABI. None of
  the devices use persistent state blobs yet.
- Display state is rebuilt when the engine rebuilds a device instance.

## Tests

`tests/limiter.rs` covers the limiter spec, panel draw list, lookahead,
threshold limiting, release recovery, and parameter rejection. `tests/gate.rs`
covers the gate spec, panel draw list, opening/closing behavior, and parameter
rejection. `tests/synth.rs` covers the synth spec, panel, oscillator modes,
note on/off, ADSR release, and parameter rejection. `notwig-ext/tests` also
loads the built WASM module and native shared library and checks the devices
through the real ABIs.
