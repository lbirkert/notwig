# notwig-ext-api

Author-facing SDK and shared, render-neutral boundary types for Notwig
extensions and the UI:

- immediate-mode panel UI: `UiContext`, `UiInput`, `UiMemory`, `Sense`,
  `Response`, and the shared `button`, `knob`, `toggle`, `dropdown`, `label`,
  `section`, `divider`, and stereo-meter widgets;
- render-neutral painting: the `Drawer` primitive trait, `RecordDrawer`,
  `DrawCommand`/`DrawList`, and `paint_*` widget helpers;
- device panel frames (`PanelDraw`, `PanelCommand`, `PanelAction`) and
  `DevicePanelHost`;
- device panel draw context and theme tokens (`DeviceDrawContext` includes the
  per-frame `UiInput` and engine-measured device peaks);
- note events (`NoteEvent`, `NoteKind`) and their fixed-size ABI wire format;
- extension manifest types and ABI constants;
- reusable wasm ABI helpers: a stable bump heap, handle-keyed `DeviceStore`,
  and safe pointer/audio slice wrappers;
- `DeviceExtension` and `Extension` for registering device implementations and
  delivering note events before each processed block;
- the `#[notwig_extension]` attribute macro, re-exported from
  `notwig-ext-macros`.

This crate has no `egui`, no `wasmtime`, and no I/O. `notwig-ui` adapts the
shared UI through `EguiUi`/`EguiDrawer`; extension code records the same API
through `RecordUi`/`RecordDrawer`. Keep wire-format changes versioned through
`notwig_ext_api::ABI_VERSION` (currently `2`).

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

Panels are dynamic: the host calls `DeviceExtension::draw` once per frame with a
`UiContext`. Use shared widgets or `UiContext::interact`, keep IDs stable across
frames, and queue parameter changes through the context so the host can validate
them:

```rust
use notwig_ext_api::{DeviceExtension, UiContext, button, knob, section};

impl DeviceExtension for MyDevice {
    fn new() -> Self {
        Self { gain: 0.5 }
    }

    fn spec() -> notwig_dsp::DeviceSpec {
        gain_spec()
    }

    fn draw(&mut self, ui: &mut UiContext<'_>) {
        section(ui, "Gain");

        let rect = ui.allocate(ui.style().knob_size);
        if let Some(norm) = knob(ui, ui.id("gain"), rect, self.gain, 0.5, false, "Gain") {
            self.gain = norm;
            ui.set_param_normalized(gain_id(), norm);
        }

        if button(ui, "Reset").clicked() {
            self.gain = 0.5;
            ui.set_param_normalized(gain_id(), 0.5);
        }
    }

    fn process(&mut self, _samples: &mut [f32]) {}
}
```

Interaction uses `Sense` (`hover`, `click`, `drag`, `click_and_drag`) and a
`Response` (`hovered`, `clicked`, `double_clicked`, `dragged`, `drag_started`,
`drag_stopped`, `drag_delta`, `contains_pointer`). The extension runtime owns one
`UiMemory` per device instance, so hover, focus, and drag state do not need to be
stored by the device.

Parameter and UI changes are emitted as `PanelAction` values
(`SetParamNormalized`, `SetParamValue`, `SetCursor`, `RequestRepaint`) and
returned in `PanelDraw::actions`. The host validates them against the device
spec before applying them: parameters must exist, values must be finite, and
normalized values are clamped. The validation helpers in `DrawList::validate`
and `PanelDraw::validate` enforce finite geometry, command/action counts, path
point counts, and text size limits. Hosts must call them before rendering
extension output.

For custom drawing, `UiContext::painter()` returns a `&mut dyn Drawer` with
`fill_rect`, `stroke_rect`, `line`, `circle_filled`, `circle_stroke`, `text`,
`path`, and clip push/pop. `RecordDrawer` buffers these as `DrawCommand`s for the
wasm boundary; `notwig-ui` replays them on the host. The `paint_*` functions
draw those same primitives into an existing rect when building custom widgets.

The `abi` module provides the boilerplate every extension needs:

- `abi::alloc` / `abi::free` / `abi::packed_pointer` for stable host inputs and
  extension-owned JSON outputs;
- `abi::DeviceStore<T>` for handle-keyed device instances with a const
  constructor suitable for `static` use;
- `abi::read_param_id`, `abi::with_str`, `abi::with_bytes`, and
  `abi::with_audio_frames` for safe reads from extension linear memory.

For `wasm32-unknown-unknown` extensions, depend on `notwig-core` and
`notwig-dsp` with `default-features = false`. This keeps the UUID `v4` feature
(and its JS/getrandom requirements) out of the wasm module. See
`notwig-ext-devices` for a working example, and
[`docs/02_extensions/`](../../docs/02_extensions/) for the package, build, and
install workflow.
