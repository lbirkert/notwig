# egui extension panels

This page defines the extension panel path. Device extensions implement their
panel with the real `egui` API inside their own context; the host keeps the
sandbox, validation, focus policy, and rendering ownership.

Status: implemented for device cards (wire validation, guest runner,
WASM/native/browser transport, texture remapping, mesh compositing, pointer and
keyboard input, focus, actions, cursor, and repaint). IME, guest accessibility
trees, overlay windows, clipboard completeness, and host-font sharing remain
follow-ups.

## Decision

Passing the host `egui::Context` or `egui::Ui` across the extension boundary is
not sound:

- `egui::Context` wraps private state and `Ui` is frame-scoped; neither has a
  stable FFI layout.
- Rust has no stable ABI for these types, and the workspace forbids unsafe code
  outside the existing native-loader adapter.
- WASM extensions have separate linear memory, so a host pointer is meaningless
  inside the guest anyway.

Instead, an extension runs a private `egui::Context` in its own instance,
tessellates the frame, and returns a validated display payload: triangle meshes,
texture deltas, host actions, and focus/repaint hints. The host composites the
meshes into the device card and routes input back through a serializable input
snapshot.

This gives authors the real egui API inside their panel. It does not give them
the host widget tree, host focus order, or arbitrary host windows. The
`notwig-ui` crate exposes a small shared component set (`knob`, `volume_fader`,
stereo meters, labels, sections, toggles, dropdowns, and palette colors) so
extension panels look and behave like the host.

## Architecture

```text
extension instance (WASM or native)
  draw_egui(ui: &mut egui::Ui, meter: DeviceMeterSnapshot)
    -> private egui::Context::run
    -> Context::tessellate
    -> EguiFrame { meshes, textures, actions, focus, repaint }
    -> postcard bytes
  ------------------------------------------------
host extension loader (notwig-ext)
  decode + validate EguiFrame
  ------------------------------------------------
host UI (notwig-ui)
  upload texture deltas into a per-panel texture namespace
  remap guest TextureId -> host TextureId
  add meshes under panel-local clips
  apply PanelAction values through the normal parameter path
  request repaint / cursor from the frame
```

The host never holds a guest pointer after the call. The guest never receives a
host pointer. Only the serialized frame and input snapshot cross the boundary.

## Capability and ABI

- Manifest capability: `ui.egui`.
- Macro: `#[notwig_extension]` always generates the egui panel export. The
  optional `egui` attribute is accepted for source compatibility.
- Exports:
  - `notwig_device_draw_egui(handle, context_ptr, context_len) -> pointer/len`
- The former command-panel export (`notwig_device_draw`) and the
  `supports_egui` query were removed in ABI version 3.

Frames use `postcard`, not JSON. A first frame can include a ~1 MiB font atlas
and mesh data; JSON would inflate both.

`EguiFrame` is validated before the host uses any of it:

- action/mesh/texture counts and bytes are capped;
- clip rects and vertex positions must be finite;
- indices are in range and a multiple of three;
- texture dimensions and total texture texels are capped;
- callbacks and `TextureId::User` are rejected;
- texture ids are panel-local and remapped by the host;
- frame bytes are read through the existing packed-pointer ABI with a separate
  egui size budget.

## Guest API

```rust
use notwig_core::dsp::DeviceSpec;
use notwig_ext_api::{
    notwig_extension, DeviceExtension, DeviceMeterSnapshot, egui, egui_panel,
};
use notwig_ui::{knob, section};

struct MyDevice { gain: f32 }

impl DeviceExtension for MyDevice {
    fn new() -> Self { Self { gain: 0.5 } }
    fn spec() -> DeviceSpec { /* ... */ }

    fn draw_egui(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
        section(ui, "My device");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(78.0, 58.0), egui::Sense::hover());
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
            egui_panel::set_param_normalized(ui, my_param_id(), norm);
        }
        let _ = meter;
    }

    fn process(&mut self, _samples: &mut [f32]) {}
}

#[notwig_extension]
pub fn extension(extension: &mut notwig_ext_api::Extension) {
    extension.register_device::<MyDevice>();
}
```

`draw_egui` runs in the guest's own context. It may use `Grid`, `ScrollArea`,
`TextEdit`, custom `Widget` implementations, and any other egui API because all
of that executes inside the guest. It must not rely on host windows, host
clipboards, or host accessibility.

Parameter changes must go through `set_param_normalized` / `set_param_value`.
The host validates the `ParamId` and value against the device spec. The guest
cannot invent parameters.

The guest runner supplies a Notwig-derived `egui::Visuals` from
`DeviceDrawContext::theme` so panels visually match the host. Text fonts are the
extension's egui default fonts; the first frame transfers the font atlas.

egui may call the draw closure more than once in one frame after
`Context::request_discard` (for example on the first frame of a `Grid`). Keep
view-state mutations idempotent, or avoid requesting a discard in device panels.

## Host compositing

`notwig-ui` owns the per-panel texture namespace:

1. Apply `textures.set` deltas with `Context::load_texture`,
   `TextureHandle::set`, or `TextureHandle::set_partial`.
2. Map each guest `Managed(n)` id to the corresponding `TextureHandle::id()`.
3. Translate mesh vertices and clip rects by the panel origin, intersect clips
   with the panel rect, and add `egui::Shape::mesh` through a clipped painter.
4. Drop handles named by `textures.free`.
5. Apply frame actions through the existing parameter validation path.
6. Forward the frame's cursor and repaint delay.

Guest id `0` is the guest font atlas, including solid-color geometry that uses
the font atlas white texel. The host maps it like any other texture.

## Input, focus, repaint

`notwig-ui` builds an `EguiInput` snapshot from the host frame:

- panel-local pointer position, pointer buttons, scroll, modifiers;
- key press/release events using egui's stable key names;
- text input and copy/cut/paste;
- panel size, `pixels_per_point`, time, and max texture side.

The host only forwards keyboard/text events while the panel owns focus. The
panel takes focus on click and releases it through normal egui interaction.
`EguiFrame::wants_keyboard` tells the shell whether to suppress host shortcuts
while the panel is focused.

Repaint comes from the guest frame's root viewport `repaint_delay`. The host
calls `Context::request_repaint_after`; the guest `Context::request_repaint`
only affects the guest, so this forwarding is required for animations and
meters.

## Explicit non-goals for this milestone

- Floating guest `Window`s or `Area`s outside the device card. Guest output is
  clipped to the card; a future overlay protocol can lift that.
- IME composition. Text input works, but CJK/IME needs `PlatformOutput.ime`
  translation and `Event::Ime` forwarding.
- Guest accessibility trees. Host-owned generic parameter controls remain the
  accessible fallback until a tree protocol exists.
- Arbitrary host UI. Extensions cannot add widgets to host layouts or host
  focus order.
- Host font sharing. Guest panels use egui's default fonts; sending the host
  `FontDefinitions` is a follow-up.
- Rust type sharing. `egui` versions must match the SDK version used by the
  extension; the wire payload is intentionally independent of egui's private
  serde layout.

## Transport coverage

The same serialized protocol is used by all three host transports:

- native WASM (`wasmtime` core module);
- native shared libraries (`libloading`, trusted only);
- browser WebAssembly (JS `WebAssembly.Instantiate`).

Native shared-library extensions remain trusted and in-process. WASM remains
sandboxed and fuel-limited. The host validates frames in both cases.

## Implementation map

`notwig-ext-api`:

- `src/egui_panel/mod.rs`: wire types, limits, validation.
- `src/egui_panel/bridge.rs`: `EguiInput -> RawInput`, `FullOutput ->
  EguiFrame`, theme mapping, parameter-action helpers.
- `src/extension.rs`: `DeviceExtension::draw_egui`, per-slot egui context and
  frame buffer.
- `src/manifest.rs`: `ui.egui` capability.
- `src/abi.rs`: ABI version and export names.

`notwig-ext-macros`:

- generate `notwig_device_draw_egui` for WASM and native targets.

`notwig-ext`:

- `abi.rs`, `native.rs`, `web/instance.rs`: egui exports, frame budget, decode
  and validation.
- `instance.rs`, `panel.rs`, `web/panel.rs`: route capable extensions through
  the egui panel path.

`notwig-ui`:

- `components/`: shared controls and meters used by both the app and guests.
- `views/device_editor/egui_render.rs`: input snapshot, focus, texture
  namespace, mesh compositing, actions.
- `views/device_editor/extension.rs`: build the draw context and composite the
  returned frame.

## Tests

- Wire round-trip and limits in `notwig-ext-api/tests/egui_wire.rs`.
- Guest frame generation and action assertions in
  `notwig-ext-api/tests/egui_bridge.rs`.
- Device-level frames in `notwig-ext-devices/tests/`.
- Host compositing, meter delivery, and invalid-frame handling in
  `notwig-ui/tests/extension_panel.rs`.
- WASM host transport in `notwig-ext/tests/abi.rs` and
  `notwig-ext/tests/limiter_extension.rs`.

## Rollout

1. Wire types, validation, and capability.
2. Guest runner and macro export.
3. Host transport and UI compositing for device cards.
4. Shared `notwig-ui` components for extension panels.
5. Tests and documentation.
6. Follow-ups: IME, clipboard completeness, accessibility tree, overlay
   windows, and per-frame performance budgets on `wasmtime`.
