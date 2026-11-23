# Panel API

`DeviceExtension::draw` runs once per UI frame while the device panel is
visible. The extension runs a private `egui::Context`; the host provides the
latest engine meter snapshot and composites the returned frame.

```rust
use notwig_ext_api::{DeviceMeterSnapshot, egui, panel_frame};
use notwig_ui::{knob, section};

fn draw(&mut self, ui: &mut egui::Ui, meter: DeviceMeterSnapshot) {
    section(ui, "Gain");

    let (rect, _) = ui.allocate_exact_size(egui::vec2(78.0, 58.0), egui::Sense::hover());
    if let Some(norm) = knob(
        ui,
        ui.make_persistent_id("gain"),
        rect,
        gain_norm(self.gain_db),
        gain_norm(0.0),
        true,
        "Gain",
    ) {
        self.gain_db = -24.0 + f64::from(norm) * 48.0;
        self.update_gain();
        panel_frame::set_param_normalized(ui, gain_id(), norm);
    }
}
```

The device keeps its own state. The extension runtime retains the guest egui
context, focus, and interaction state per device instance.

## Shared components

Use the components from `notwig-ui` so extension panels match the host:

| Component | Purpose |
| --- | --- |
| `knob(ui, id, rect, value_norm, default_norm, bipolar, tooltip)` | Rotary control; returns the new normalized value when changed. |
| `volume_fader(ui, id, rect, db, min_db, max_db)` | Horizontal decibel fader; returns the new value when changed. |
| `draw_stereo_meter(painter, rect, peak_l, peak_r)` | Stereo peak meter with the shared -60 dB floor. |
| `draw_stereo_meter_with_peak(painter, rect, peak_l, peak_r, peak_hold)` | Same meter with a peak-hold marker. |
| `button(ui, text)` | Standard themed button. |
| `label(ui, text)` | Muted label. |
| `section(ui, title)` | Compact uppercase section heading. |
| `divider(ui)` | Horizontal separator. |
| `toggle(ui, active, text)` | Selectable toggle; returns whether it was clicked. |
| `dropdown(ui, id, selected, options)` | Dropdown; returns the newly selected index. |
| `format_param(unit, value)`, `format_db(db)` | Shared value formatting. |

For custom drawing, use `ui.painter()` with normal egui shapes. The shared
`notwig_ui::palette` module exposes the host colors (`ACCENT`, `PANEL`, `GRID`,
`TEXT_MUTED`, and so on) for graphs and custom meters.

Knobs change by vertical drag. Hold **Shift** for fine adjustment. Double-click
resets to the default; Cmd/Ctrl+click opens the shared numeric editor.

## Parameter actions

Drawing code cannot mutate host state directly. Parameter changes are queued as
`PanelAction` values and returned inside `PanelFrame::actions`:

- `SetParamNormalized { param_id, norm }`
- `SetParamValue { param_id, value }`
- `SetCursor { cursor }`
- `RequestRepaint`

Use `panel_frame::set_param_normalized` for knobs and drag controls. Use
`panel_frame::set_param_value` when you already have a typed value, such as an
enum index or a reset to a known default.

The host validates actions against the device spec before applying them:

- the parameter must exist on the device spec;
- values must be finite;
- normalized values are clamped to `0..=1`;
- exact values must match the parameter kind and are clamped or rounded to the
  spec range and step;
- actions for the same parameter in one frame apply in order, and the last one
  wins;
- invalid actions are dropped, and actions are capped by `MAX_PANEL_ACTIONS`.

## Metering

`draw` receives the engine-measured `DeviceMeterSnapshot`:

```rust
let to_linear = |db: f32| 10.0_f32.powf(db / 20.0);
draw_stereo_meter_with_peak(
    ui.painter(),
    meter_rect,
    to_linear(self.meter_left_db),
    to_linear(self.meter_right_db),
    Some(to_linear(self.meter_peak_db)),
);
```

The fields are linear peak amplitudes: `input_peak_l`, `input_peak_r`,
`output_peak_l`, and `output_peak_r`. Use the ratio of output to input to show
device gain reduction. Values are zero while playback is stopped. Retain display
history on the device, feed it from the snapshot, and call
`ui.ctx().request_repaint()` while the panel is visible so meters animate.

## Interaction and layout

`draw` gets the real egui `Ui`. Use normal egui layout (`ui.horizontal`,
`ui.vertical`, `ui.add_space`, `ui.allocate_exact_size`, `Grid`, `ScrollArea`)
and normal egui widgets. Keep IDs stable across frames with
`ui.make_persistent_id(...)`.

For controls that need an explicit rectangle, allocate the rectangle yourself
and pass it to the shared component:

```rust
let (rect, _) = ui.allocate_exact_size(egui::vec2(78.0, 58.0), egui::Sense::hover());
let changed = knob(ui, ui.make_persistent_id("gain"), rect, norm, default, false, "Gain");
```

## Focus and repaint

The host forwards pointer and keyboard input while the panel is focused. The
guest frame reports `wants_keyboard` so the shell can suppress host shortcuts.
Repaint timing comes from the guest root viewport and is forwarded to the host;
call `ui.ctx().request_repaint()` for animations and meters.

## Limits

Frame and package limits are enforced by both hosts. Resource limits are
enforced only by the native wasmtime host:

| Limit | Value | Enforced by |
| --- | --- | --- |
| entrypoint file size | 64 MiB | native and browser |
| WASM linear memory | 64 MiB | native wasmtime only |
| WASM fuel | 20,000,000 per call | native wasmtime only |
| egui primitives per frame | 8192 | native and browser |
| vertices per frame | 262,144 | native and browser |
| indices per frame | 786,432 | native and browser |
| textures per frame | 256 | native and browser |
| texture side | 8192 px | native and browser |
| serialized panel frame | 8 MiB | native and browser |
| actions per frame | 256 | native and browser |

The browser host uses the platform `WebAssembly` API, which provides neither
fuel metering nor a host-configurable linear-memory cap, so it relies on the
browser sandbox for compute and memory limits. It does cap `notwig.host.log`
and `notwig.host.notify` copies at 8 KiB per call.

`PanelFrame::validate()` enforces finite geometry, clip ordering, index bounds,
texture sizes, command/action counts, and the frame budget. The host calls it
before compositing.

## Native builds

Native entrypoints are not sandboxed; they run in the Notwig process and use the
same panel API with lower overhead. The host still validates the returned
`PanelFrame` before rendering. Only install native extensions from sources you
trust.

## Why not pass the host egui context

`egui::Context` and `egui::Ui` carry lifetimes and host-bound state, have no
stable FFI layout, and cannot be passed across the WASM boundary. Running a
private guest context keeps output validated, bounded, and isolated while still
giving authors the real egui API.

See [extension panels](../03_development/12_extension_panels.md) for
the protocol, limits, and current non-goals (floating windows, IME, and guest
accessibility trees).

Next: [Testing and Distribution](05_testing_and_distribution.md).
