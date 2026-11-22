# notwig-ext-api

Author-facing SDK and shared boundary types for Notwig extensions.

## Purpose

`notwig-ext-api` defines the stable device-extension contract: device specs,
guest egui panel transport, note events, manifests, target matching, ABI
constants, and the reusable ABI helpers generated into extension modules. The
host uses the same wire types to validate and composite extension panels.

## Owns

- Guest egui panel frames (`EguiFrame`, `EguiInput`, meshes, texture deltas) and
  their limits/validation.
- the `#[notwig_extension]` ABI generator, re-exported from
  `notwig-ext-macros`;
- `DeviceExtension` and `Extension` for registering device implementations and
  delivering note events before each processed block;
- note events (`NoteEvent`, `NoteKind`) and their fixed-size ABI wire format;
- extension manifest types, ordered entrypoints, target matching, and ABI
  constants;
- reusable ABI helpers: a stable bump heap, handle-keyed `DeviceStore` with
  per-instance locks, and safe WASM-offset and native-pointer wrappers.

Panel rendering is egui-only. The guest runs a private `egui::Context`; the
host only receives validated, tessellated meshes and texture deltas. There is
no render-neutral command IR. Extension panels should use the shared components
from `notwig-ui` for knobs, meters, labels, sections, toggles, and dropdowns.

This crate has no `wasmtime` and no I/O. Keep wire-format changes versioned
through `notwig_ext_api::ABI_VERSION` (currently `3`).

Device authors implement `DeviceExtension` for their device type and register it
from the entrypoint:

```rust
use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<MyDevice>();
}
```

The macro generates the WASM ABI when compiling for
`wasm32-unknown-unknown` and a native shared-library ABI on desktop targets.
Cargo requires the proc macro to live in a separate crate, so
`notwig-ext-macros` is an implementation detail re-exported here; authors only
depend on `notwig-ext-api`.

Panels are dynamic: the host calls `DeviceExtension::draw_egui` once per frame
with the guest egui `Ui` and the latest engine-measured device peaks. Queue
parameter changes through the egui panel helpers so the host can validate them:

```rust
use notwig_core::dsp::{DeviceSpec, DeviceUiSpec, ParamSpec, Unit};
use notwig_ext_api::{DeviceExtension, DeviceMeterSnapshot, egui, egui_panel};
use notwig_ui::{knob, section};

impl DeviceExtension for MyDevice {
    fn new() -> Self {
        Self { gain: 0.5 }
    }

    fn spec() -> DeviceSpec {
        gain_spec()
    }

    fn draw_egui(&mut self, ui: &mut egui::Ui, _meter: DeviceMeterSnapshot) {
        section(ui, "Gain");

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(78.0, 58.0), egui::Sense::hover());
        if let Some(norm) = knob(
            ui,
            ui.make_persistent_id("gain"),
            rect,
            self.gain,
            0.5,
            false,
            "Gain",
        ) {
            self.gain = norm;
            egui_panel::set_param_normalized(ui, gain_id(), norm);
        }
    }

    fn process(&mut self, _samples: &mut [f32]) {}
}
```

Interaction uses normal egui widgets and the shared `notwig-ui` controls. The
extension runtime owns the guest `egui::Context`, so the device only keeps its
own parameter values and view state.

Parameter and UI changes are emitted as `PanelAction` values
(`SetParamNormalized`, `SetParamValue`, `SetCursor`, `RequestRepaint`) and
returned inside `EguiFrame::actions`. The host validates them against the device
spec before applying them: parameters must exist, values must be finite, and
normalized values are clamped.

The `abi` module provides the boilerplate every extension needs:

- `abi::alloc` / `abi::free` / `abi::packed_pointer` for stable host inputs and
  extension-owned outputs;
- `abi::DeviceStore<T>` for handle-keyed device instances with a const
  constructor suitable for `static` use;
- `abi::read_param_id`, `abi::with_str`, `abi::with_bytes`, and
  `abi::with_audio_frames` for safe reads from extension linear memory.

For `wasm32-unknown-unknown` extensions, depend on `notwig-core` and
`notwig-ui` with `default-features = false`. This keeps UUID `v4` and the app
shell out of the wasm module. Native builds use the same crate and do not need a
special feature. See `notwig-ext-devices` for working examples, and
[`docs/02_extensions/`](../../docs/02_extensions/) for the package, build, and
install workflow.
