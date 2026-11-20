# notwig-ext-devices

Example extension devices for Notwig, built as a core wasm module against the
`notwig-ext-api` contract.

## Devices

### Limiter

`notwig.ext.devices.limiter`

A simple stereo-linked peak limiter:

- **Threshold**: -24 dB to 0 dB, default -6 dB.
- **Release**: 0.01 s to 0.5 s, default 0.1 s.
- Instant attack, release-smoothed gain recovery, zero reported latency.

The custom panel is a `PanelDraw` with two native Notwig knobs. DSP runs over
interleaved `f32` samples in extension linear memory. The manifest also includes
a short description and `icon.png` for the extension explorer.

The device is implemented with the SDK trait and registered from the macro
entrypoint:

```rust
use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
}
```

`LimiterDevice` implements `DeviceExtension`, providing `spec`, `panel`,
`prepare`, `set_param`, `process`, and `latency`. The generated ABI exports and
all heap/store/pointer plumbing come from `notwig-ext-api`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build -p notwig-ext-devices --target wasm32-unknown-unknown --release
```

The module is written to:

```text
target/wasm32-unknown-unknown/release/notwig_ext_devices.wasm
```

## Install

Copy the manifest and module into a directory under the user extensions
directory:

```text
~/Library/Application Support/notwig/extensions/notwig.ext.devices/
  extension.json
  notwig_ext_devices.wasm
```

Then open **Settings > Extensions** and press **Reload extensions**. The
Limiter appears under **Extensions** in the Add Device browser.

## Notes

- Heap, device-store, pointer/audio helpers, and the wasm exports are generated
  by the SDK and `#[notwig_extension]`. This crate contains only the limiter
  implementation and its panel/spec.
- `notwig_alloc` uses the SDK's fixed bump heap so pointers stay stable for the
  lifetime of the module instance.
- The extension crate does not enable `unsafe`; generated code keeps its
  `no_mangle` attributes inside an SDK-generated module.
- The limiter does not need note events or persistent state blobs yet.

## Tests

`tests/limiter.rs` checks the serialized device spec, the panel draw list,
threshold limiting, release recovery, and the create/prepare/destroy ABI path.
