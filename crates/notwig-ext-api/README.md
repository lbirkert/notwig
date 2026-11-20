# notwig-ext-api

Author-facing SDK and shared, render-neutral boundary types for Notwig
extensions and the UI:

- geometry and color values (`Point`, `Rect`, `Color`, `Stroke`);
- immediate-mode drawing commands (`DrawList`, `DrawCommand`);
- device panel commands (`PanelDraw`, `PanelCommand`) including host-rendered
  parameter knobs and toggles;
- device panel draw context and theme tokens;
- extension manifest types and ABI constants;
- reusable wasm ABI helpers: a stable bump heap, handle-keyed `DeviceStore`,
  and safe pointer/audio slice wrappers;
- `DeviceExtension` and `Extension` for registering device implementations;
- the `#[notwig_extension]` attribute macro, re-exported from
  `notwig-ext-macros`.

This crate has no `egui`, no `wasmtime`, and no I/O. `notwig-ui` translates draw
and panel commands into `egui`; `notwig-ext` uses the same types across
the wasm boundary. Keep wire-format changes versioned through
`notwig_ext_api::ABI_VERSION`.

Device authors implement `DeviceExtension` for their device type and register it
from the entrypoint:

```rust
use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<MyDevice>();
}
```

The macro generates the wasm ABI exports around that entrypoint. Cargo requires
the proc macro to live in a separate crate, so `notwig-ext-macros` is an
implementation detail re-exported here; authors only depend on
`notwig-ext-api`.

The `abi` module provides the boilerplate every extension needs:

- `abi::alloc` / `abi::free` / `abi::packed_pointer` for stable host inputs and
  extension-owned JSON outputs;
- `abi::DeviceStore<T>` for handle-keyed device instances with a const
  constructor suitable for `static` use;
- `abi::read_param_id`, `abi::with_str`, and `abi::with_audio_frames` for safe
  reads from extension linear memory.

`DrawList::validate` and `PanelDraw::validate` enforce finite geometry, command
counts, path point counts, and text size limits. Hosts must call them before
rendering extension output.

For `wasm32-unknown-unknown` extensions, depend on `notwig-core` and
`notwig-dsp` with `default-features = false`. This keeps the UUID `v4` feature
(and its JS/getrandom requirements) out of the wasm module. See
`notwig-ext-devices` for a working example.
