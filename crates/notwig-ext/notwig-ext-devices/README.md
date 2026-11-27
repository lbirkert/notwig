# notwig-ext-devices

Example extension devices for Notwig, built against the `notwig-ext-api`
contract. The same crate produces a portable WASM module and a native shared
library. All panels use the guest egui API and shared `notwig-ui` components.

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

### Compressor

`notwig.ext.devices.compressor`

A stereo-linked compressor with a soft knee:

- **Threshold**, **Ratio**, **Attack**, **Release**, **Knee**, **Makeup**, and
  **Mix**.
- Peak detector with attack/release-smoothed gain reduction, soft-knee curve,
  and smoothed make-up gain.
- The shared peak/gain history graph and stereo output meter.

### Clipper

`notwig.ext.devices.clipper`

A soft clipper with tone control:

- **Drive**, **Knee** (hard-to-soft), **Tilt**, **Output**, and **Mix**.
- Soft clipping uses a blend between hard clip and `tanh`, followed by a DC
  blocker and a simple low/high tilt EQ.
- A transfer-curve preview and stereo meter.

### Delay

`notwig.ext.devices.delay`

A stereo delay:

- **Time L**, **Time R**, **Feedback**, **Crossfeed**, **Damping**,
  **Ducking**, **Width**, and **Mix**.
- Two fractional delay lines, damping in the feedback path, crossfeed for
  ping-pong-style repeats, and dry-input ducking. Free time only; tempo sync is
  not available in the current extension ABI.
- A stereo meter and two compact knob rows.

### Chorus

`notwig.ext.devices.chorus`

A stereo chorus/flanger:

- **Mode** (Chorus/Flanger), **Rate**, **Depth**, **Feedback**, **Delay**,
  **Width**, and **Mix**.
- Two modulated delay lines with sine/triangle LFOs, stereo phase spread,
  feedback damping, and a live LFO graph.

### Transient

`notwig.ext.devices.transient`

A transient shaper:

- **Attack Gain/Time**, **Sustain Gain/Time**, **Mode** (Peak/RMS), and **Mix**.
- Fast and slow stereo-linked envelope followers detect the transient and body;
  attack/sustain gains are blended by the transient amount and smoothed.
- An envelope preview and stereo meter.

### Reverb

`notwig.ext.devices.reverb`

A stereo algorithmic reverb with a wet FX chain:

- **Type**: Room, Hall, or Plate.
- **Size**: 0 to 1, default 0.6.
- **Damping**: 0 to 1, default 0.4.
- **Pre-delay**: 0 s to 0.2 s, default 0.02 s.
- **Width**: 0 to 1, default 1.0.
- **Mix**: 0 to 1, default 0.3.
- A dense network of pre-diffusion allpasses, 8 damped comb filters, and 4
  post-allpasses per channel. The Room/Hall/Plate types change comb/allpass
  lengths, feedback, damping, and diffusion without allocating in `set_param`.
- Declared as `DeviceKind::Container` with `ContainerRouting::StageThenChildren`
  and `Mix` as the container wet parameter. The host owns the child chain, so
  nested devices process the wet reverb signal before the dry/wet blend.
- A type-aware decay graph, stereo output meter, and shared knobs.

### Synth

`notwig.ext.devices.synth`

A simple polyphonic synthesizer:

- **Oscillator**: Sine, Square, Saw, or Triangle.
- **Volume ADSR**: Attack, Decay, Sustain, and Release.
- 16 voices with pitch-matched note on/off handling and a fixed per-voice gain.
- Receives note events through the extension note-event ABI.

The panel uses the shared `notwig_ui::dropdown` component for the oscillator and
four shared knobs for the ADSR.

### Gain

`notwig.ext.devices.gain`

A small gain device that demonstrates the guest panel path:

- draws a shared `notwig-ui` knob and labels;
- reports parameter changes through `panel_frame::set_param_value`;
- is registered with the same `#[notwig_ext(load)]` entrypoint as the others.

All devices draw dynamically each frame with `DeviceExtension::draw` and
request repaint so they animate while visible. Meter and history data come from
the engine-measured `DeviceMeterSnapshot` passed to `draw`, because the
panel runs in a separate extension instance from the audio processor. The graph
and history ring are local to this crate and shared by the limiter and gate.

The devices are implemented with the SDK trait and registered from the macro
entrypoint:

```rust
use notwig_ext_api::{EP_NOTWIG_DEVICES, DeviceExtension, ExtensionBuilder, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<LimiterDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<CompressorDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<ClipperDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<DelayDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<ChorusDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<TransientDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<GateDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<ReverbDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<SynthDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension::<GainDevice>(EP_NOTWIG_DEVICES);
}
```

Each device implements `DeviceExtension`, providing `spec`, `draw`,
`prepare`, `set_param`, `process`, and `latency`. The extension runtime retains
the guest egui context; the devices keep only their own parameter values and
display state. The generated ABI exports and all heap/store/pointer plumbing
come from `notwig-ext-api`.

## Module map

| Module | Responsibility |
| --- | --- |
| `lib.rs` | Wiring and `load`/`unload` lifecycle entrypoints only |
| `devices.rs` | Device module wiring, re-exports, and `EP_NOTWIG_DEVICES` registrations |
| `synth/mod.rs` | Synth device state and `DeviceExtension` delegation |
| `synth/params.rs` | Synth parameter ids, ranges, defaults, normalization |
| `synth/spec.rs` | Synth device spec |
| `synth/panel.rs` | Synth egui panel |
| `synth/dsp.rs` | Oscillators, voices, envelopes, note handling, processing |
| `gate/mod.rs` | Gate device state and `DeviceExtension` delegation |
| `gate/params.rs` | Gate parameter ids, ranges, defaults, normalization |
| `gate/spec.rs` | Gate device spec |
| `gate/panel.rs` | Gate egui panel |
| `gate/dsp.rs` | Gate detector, meter ballistics, and processing |
| `limiter.rs` | Limiter DSP, spec, panel, meters, parameter ids |
| `gain.rs` | Small guest-egui gain example |
| `reverb.rs` | Stock reverb device |
| `dynamics.rs` | Shared peak/gain history and history-graph painter |
| `ui.rs` | Shared knob caption helper |

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

To build and package the stock extension locally:

```sh
cargo run -p cargo-notwig-ext -- build --entrypoint wasm --path crates/notwig-ext/notwig-ext-devices
cargo run -p cargo-notwig-ext -- build --package wasm --path crates/notwig-ext/notwig-ext-devices --out-dir docs/extensions --name ext-devices-wasm
```

## Install

Install the generated ZIP from **Settings > Extensions > Add .ZIP**. The host
can also load a manually unpacked package from the user extensions directory:

```text
~/Library/Application Support/notwig/extensions/notwig.ext.devices/
  extension.json             # packaged manifest with entrypoints
  notwig_ext_devices.dylib   # native target, optional
  notwig_ext_devices.wasm
  icon.png
```

Restart Notwig after unpacking the package. The devices appear under
**Extensions** in the Add Device browser.

The source manifest uses `"api_version": 8` and the `ui.panel` capability. It
omits `entrypoints`; the CLI adds the selected target entrypoint to the packaged
manifest.

## Notes

- Heap, device-store, pointer/audio helpers, and the ABI exports are generated
  by the SDK and `#[notwig_ext(load)]`. This crate contains only the device
  implementations and their spec/panel code.
- WASM uses the SDK's fixed bump heap so pointers stay stable for the lifetime
  of the module instance. Native builds pass process pointers directly.
- The extension crate does not enable `unsafe`; generated code keeps its
  `no_mangle` attributes inside an SDK-generated module.
- The synth receives note events through the extension note-event ABI. None of
  the devices use persistent state blobs yet.
- The reverb is a container. The host runs its child chain on the wet path and
  applies the `Mix` parameter after the reverb and its nested devices; the
  reverb's `process` outputs wet only.
- Display state is rebuilt when the engine rebuilds a device instance.

## Tests

`tests/limiter.rs` covers the limiter spec, panel frame generation, lookahead,
threshold limiting, release recovery, and parameter rejection. `tests/gate.rs`
covers the gate spec, frame generation, opening/closing behavior, and parameter
rejection. `tests/reverb.rs` covers the container spec, frame generation,
pre-delay, Room/Hall/Plate tails, reset behavior, and parameter rejection.
`tests/compressor.rs`, `tests/clipper.rs`, `tests/delay.rs`, `tests/chorus.rs`,
and `tests/transient.rs` cover the new effects. `tests/synth.rs` covers the
synth spec, frame generation, oscillator modes, note on/off, ADSR release, and
parameter rejection.
`tests/gain_device.rs` covers the gain device frame and parameter behavior.
`crates/notwig-ext/notwig-ext/tests` also loads the built WASM module and native shared library
and checks the devices through the real ABIs.
