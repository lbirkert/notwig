# Panel API

`DeviceExtension::draw` runs once per UI frame while the device panel is
visible. It uses `UiContext`, an immediate-mode API that owns layout,
interaction, painting, and host actions for that frame.

```rust
fn draw(&mut self, ui: &mut UiContext<'_>) {
    section(ui, "Gain");

    ui.horizontal(|ui| {
        let rect = ui.allocate(ui.style().knob_size);
        if let Some(norm) = knob(
            ui,
            ui.id("gain"),
            rect,
            gain_norm(self.gain_db),
            gain_norm(0.0),
            true,
            "Gain",
        ) {
            self.gain_db = -24.0 + f64::from(norm) * 48.0;
            self.update_gain();
            ui.set_param_normalized(gain_id(), norm);
        }
    });

    divider(ui);
    label(ui, "Double-click the knob to reset.");
}
```

The device keeps its own state. Interaction memory (hover, focus, drag) is
retained by the host per device instance.

## Layout and context

| Method | Purpose |
| --- | --- |
| `allocate(size)` | Reserve a rect at the layout cursor and advance it. |
| `available_rect()` | Remaining panel-local space. |
| `horizontal(add)` / `vertical(add)` | Run a sub-layout in one direction. |
| `interact(id, rect, sense)` | Register hit-testing and return a `Response`. |
| `id(source)` | Build a stable `UiId` from a hash and the current ID stack. |
| `push_id` / `pop_id` | Namespace repeated widgets. |
| `painter()` | Primitive `Drawer` for custom drawing. |
| `push_clip` / `pop_clip` | Restrict painting and hit-testing to a rect. |
| `set_param_normalized` / `set_param_value` | Queue a parameter action. |
| `set_cursor` / `request_repaint` | Queue host UI actions. |
| `style()`, `theme()`, `input()` | Style metrics, theme colors, frame input. |
| `device_meter()` | Latest engine-measured input/output peaks for this device. |
| `font()`, `mono_font()`, `text_size()`, `small_text_size()` | Font requests. |

`UiContext` never exposes `egui`. The host sends a serialized `UiInput` and
replays the resulting `PanelDraw`.

## Widgets

Shared widgets allocate their own rect and draw themselves:

| Widget | Returns |
| --- | --- |
| `button(ui, text)` | `Response` |
| `knob(ui, id, rect, value_norm, default_norm, bipolar, label)` | `Option<f32>` when the normalized value changed |
| `toggle(ui, id, rect, active, text)` | `Option<bool>` when clicked |
| `dropdown(ui, id, rect, selected, options, open)` | `Option<usize>` when an item is chosen; `open` is caller-owned popup state |
| `stereo_meter(ui, size, peak_left, peak_right, peak_hold)` | Allocated `Rect` |
| `label(ui, text)`, `section(ui, title)`, `divider(ui)` | Allocated `Rect` |

The `paint_*` functions (`paint_button`, `paint_knob`, `paint_toggle`,
`paint_label`, `paint_section`, `paint_divider`, `paint_stereo_meter`,
`paint_dropdown`) draw a widget into an existing rect when you need custom
layout or interaction.

`dropdown` keeps its open state in a `bool` owned by the device. Allocate the
header rect during layout, then call `dropdown` after the rest of the panel so
its option list paints and hit-tests above other widgets. Clicking outside the
list closes it.

`stereo_meter` takes linear peak amplitudes (`0.0..`). It maps them to dBFS with
the same -60 dB floor as the host track meters, draws green/amber/red zones, and
adds a peak-hold line from the linear `peak_hold` value. `METER_MIN_DB` exposes
that floor.

### Device metering

The engine measures each device's input and output peaks every block and passes
them to the panel through `ui.device_meter()`:

```rust
let meter = ui.device_meter();
let input = meter.input_peak_l.max(meter.input_peak_r);
let output = meter.output_peak_l.max(meter.output_peak_r);
```

Each field is a linear peak amplitude. The values are zero while playback is
stopped or when the device is inactive. Use them to drive a `stereo_meter` and
to build display history such as a limiter graph. The panel instance itself does
not process audio, so do not expect local DSP state to contain meter data.

Retain display history on the device and call `ui.request_repaint()` while the
panel is visible so meters animate.

The knob changes by vertical drag. Hold **Shift** for fine adjustment.
Double-clicking a knob resets it to `default_norm`.

## Interaction

`Sense` declares which pointer interactions a rect accepts:

- `Sense::hover()`
- `Sense::click()` (includes hover)
- `Sense::drag()` (includes hover)
- `Sense::click_and_drag()` (includes hover)

`Response` reports `hovered`, `clicked`, `double_clicked`, `dragged`,
`drag_started`, `drag_stopped`, `drag_delta`, and `contains_pointer`. React in
the same frame:

```rust
if button(ui, "Reset").clicked() {
    self.gain_db = 0.0;
    self.update_gain();
    ui.set_param_value(gain_id(), ParamValue::F64(0.0));
}
```

Choose stable IDs with `ui.id("...")`, or wrap repeated controls with
`push_id`/`pop_id`. IDs are not persisted; they only need to stay stable across
frames so hover and drag state is retained.

## Actions

Drawing code cannot mutate host state directly. Parameter and UI changes are
queued as `PanelAction` values and returned in `PanelDraw::actions`:

- `SetParamNormalized { param_id, norm }`
- `SetParamValue { param_id, value }`
- `SetCursor { cursor }`
- `RequestRepaint`

The host validates actions against the device spec before applying them:

- the parameter must exist on the device spec;
- values must be finite;
- normalized values are clamped to `0..=1`;
- exact values must match the parameter kind and are clamped or rounded to the
  spec range and step;
- actions for the same parameter in one frame apply in order, and the last one
  wins;
- invalid actions are dropped, and actions are capped by `MAX_ACTIONS`.

Use `set_param_normalized` for knobs and drag controls. Use `set_param_value`
when you already have a typed value, such as a reset to a known default.

## Custom drawing

When the shared widgets are not enough, paint through `ui.painter()`:

```rust
use notwig_ext_api::{Color, Point, Size, Stroke};

let rect = ui.allocate(Size::new(200.0, 40.0));
let theme = ui.theme();
let painter = ui.painter();
painter.fill_rect(rect, 4.0, theme.panel_light);
painter.line(
    Point::new(rect.min.x, rect.center().y),
    Point::new(rect.max.x, rect.center().y),
    Stroke::new(1.0, theme.grid),
);
```

`Drawer` supports:

- `fill_rect`
- `stroke_rect`
- `line`
- `circle_filled`
- `circle_stroke`
- `text`
- `path`
- `push_clip` / `pop_clip`

Each call records a `DrawCommand`. `RecordDrawer` buffers commands on the
extension side. `notwig-ui` replays them through `EguiDrawer` on the host.
Native builds use the same command stream.

Coordinates are panel-local pixels. Use `ui.theme()` for colors,
`ui.font()`/`ui.mono_font()` for font requests, and `ui.center_align()` or
`notwig_ext_api::Align` for text anchors.

## Limits

Host limits are enforced on the WASM boundary:

| Limit | Value |
| --- | --- |
| entrypoint file size | 64 MiB |
| WASM linear memory | 64 MiB |
| WASM fuel | 20,000,000 per call |
| panel commands per frame | 4096 |
| path points per command | 1024 |
| text bytes per item | 8 KiB |
| serialized panel output | 256 KiB |
| actions per frame | 256 |

`PanelDraw::validate()` enforces finite geometry, command/action counts, path
limits, text limits, and action shape. The host calls it before rendering.

## Native builds

Native entrypoints are not sandboxed; they run in the Notwig process and can use
the same panel API with lower overhead. The host still validates the returned
`PanelDraw` before rendering. Only install native extensions from sources you
trust.

## Why not egui directly

`egui::Context` and `egui::Ui` carry lifetimes and host-bound state, have no
stable FFI layout, and cannot be passed across the WASM boundary. The typed
`UiContext`/`Drawer`/`PanelAction` surface keeps output themed, validated, and
bounded.

## Opt-in guest egui panels

Extensions that need the real egui API can declare `ui.egui`, enable the SDK
`egui` feature, and implement `DeviceExtension::draw_egui`. The extension runs a
private `egui::Context`, tessellates its frame, and returns meshes, texture
deltas, and validated actions. The host remaps textures, composites the meshes
inside the device card, and routes input, focus, and repaint back to the guest.
The host never passes its `egui::Context` or `Ui` into the extension.

See [egui extension panels](../03_development/12_egui_extension_panels.md) for
the protocol, limits, and current non-goals (floating windows, IME, and guest
accessibility trees).

Next: [Testing and Distribution](05_testing_and_distribution.md).
