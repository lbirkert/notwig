# Extension drawing API

Status: design proposal. This note records the options considered and the
recommended path for letting WASM extensions draw and interact with device
panels using an egui-like immediate-mode API while sharing primitives and
widgets with `notwig-ui`.

## Short recommendation

The extension-facing API should be a `UiContext`, not just a `Drawer`. `Drawer`
remains the primitive painting layer underneath it, but `UiContext` is what
extension authors program against: allocate, layout, draw, hit-test, react to
clicks, and emit parameter changes. That matches how egui code is written and
is the part that needs to be future-proof.

Do not pass `egui::Context` or `egui::Ui` into WASM. Instead, put a neutral
immediate-mode UI layer in `notwig-ext-api::ui`:

- `Drawer`: primitive painting (`fill_rect`, `line`, `text`, `path`, clips).
- `UiContext`: the ergonomic, egui-like facade that owns layout, IDs, widget
  composition, and action recording.
- `UiBackend`: the small low-level trait implemented twice:
  - `RecordUi` for WASM extensions: reads an input snapshot, maintains
    interaction memory, and records commands plus actions.
  - `EguiUi` for the host: delegates interaction and cursor handling to
    `egui::Ui`, and paints through `EguiDrawer`.
- Shared widget functions written once against `UiContext`: `button`, `knob`,
  `toggle`, `slider`, `label`, `section`, `meter`, and so on.

Extensions never see egui. They redraw from a per-frame context, react to
`Response` values exactly like egui (`if ui.button("Reset").clicked() { ... }`),
and return a command list plus validated actions through
`notwig_device_draw`.

The important distinction is:

- **Drawing** is `Drawer`.
- **Interaction and composition** are `UiContext`.
- **Host adaptation** is `UiBackend` (`RecordUi` vs `EguiUi`).

`Drawer` alone was not enough because it cannot handle hover, clicks, drag
capture, double-clicks, focus, or immediate-mode reactions. `UiContext` is the
future-proof contract.

## Constraints

- `notwig-ext-api` must stay egui-free. `docs/ARCHITECTURE.md` lists `egui` as
  something the SDK crate must not depend on, and the same crate must compile
  to `wasm32-unknown-unknown` for extension authors.
- `notwig-ui` already depends on `notwig-ext-api`, so the neutral layer can
  live there without adding a new crate or a cycle.
- The render-neutral layer must not depend on `wasmtime`, the app, or any host
  state. It is plain data and traits.
- The browser-build plan expects the extension ABI and draw IR to be reusable
  with the browser `WebAssembly` API later, so the wire format cannot contain
  host-only types.
- Device-panel rendering and interaction run on the UI thread. Nothing in this
  design touches the audio callback.

## Why not pass egui to WASM

`egui::Context` and `egui::Ui` are host Rust objects. They contain lifetimes,
thread-affine state, allocator-owned collections, font atlases, texture
handles, and callback closures. There is no stable FFI layout for them, and a
WASM module cannot hold a host pointer safely. Passing one would also let an
extension mutate arbitrary host UI state.

Alternatives considered:

| Option | What it buys | Why it is not the default |
|---|---|---|
| Pass `egui::Context`/`Ui` across the boundary | Full egui API | Not representable across WASM, unsafe, version-coupled, host-state access |
| Embed `egui` inside the extension, run `Context::run`, serialize shapes | Full egui API inside the module | Every extension carries egui/epaint; shape, galley, and atlas formats are version-locked; host fonts differ; resource limits become much harder; artifact size and trust surface grow |
| Serialize egui `FullOutput`/`ClippedPrimitive` | Reuses egui painting | Same version coupling, plus texture deltas and font atlas management across the boundary |
| Render-neutral `UiContext` + `Drawer` (recommended) | egui-like authoring and interaction, stable wire format, host-owned validation | We maintain the small UI/widget layer ourselves |

If a future extension genuinely needs an unsupported widget or capability, add
it as a typed shared widget, a typed action, or a capability-gated host import.
Do not expose arbitrary egui.

## Current state and gaps

The first pass already has the right foundation:

- `notwig-ext-api::geometry` defines `Point`, `Rect`, `Color`, `Stroke`, and
  `TextAlign`.
- `notwig-ext-api::draw` defines `DrawCommand`/`DrawList` with validation.
- `notwig-ext-api::panel` defines `PanelCommand` with primitive plus
  `Knob`/`Toggle`/`Label`/`Section` commands.
- `notwig-ui/src/views/device_editor/extension.rs` validates and replays a
  `PanelDraw` into egui.
- `notwig-ext` owns the WASM instances and the panel host.

Current gaps:

1. **Drawing is static.** `DeviceExtension::panel()` is called once at
   registration, and the generated `notwig_device_draw` export ignores the
   context (`crates/notwig-ext-macros/src/lib.rs`, `_ctx_ptr`/`_ctx_len`). A
   panel cannot react to size, theme, hover, drag, or parameter changes.
2. **No input.** `DeviceDrawContext` carries params, size, and theme but no
   pointer or modifier snapshot, so extension-side immediate-mode interaction
   is impossible.
3. **Painting is duplicated.** Built-in widgets paint directly with egui, while
   extension commands are translated by a separate `paint_draw` function. There
   is no single definition of a knob, divider, meter, or label.
4. **UI instances are keyed by type ID.** `ExtensionPanelHost` stores one
   instance per `type_id`, so two devices of the same type share UI state.
5. **No action path.** An extension cannot report "the user changed this
   parameter" for a custom control. Only the host-owned knob/toggle commands
   can change parameters.

## Architecture

```text
extension panel code
        |
        v
UiContext facade               layout, IDs, widgets, immediate-mode reactions
        |
        v
UiBackend trait                allocate / interact / input / cursor / actions
        |
        +--> RecordUi --> RecordDrawer --> PanelDraw + actions -> WASM -> host
        |
        +--> EguiUi   --> EguiDrawer   --> egui::Ui / egui::Painter -> screen

shared widgets                 button / knob / toggle / slider / label / meter
        |                      built once on top of UiContext
        v
Drawer trait                   primitive painting only
```

The same widget code runs on both sides. Only the backend differs: one drives
egui, the other drives the recorder and the interaction state machine.

### Why a facade plus a backend trait

A single object-safe `UiContext` trait sounds appealing, but it breaks down
quickly:

- Layout wants closures (`ui.vertical(|ui| ...)`) and generic methods
  (`ui.id("threshold")`), which are awkward or impossible on a trait object.
- A full egui-compatible trait would be enormous, and most of it is not safe or
  meaningful across the WASM boundary.
- We want one implementation of layout and widget composition, not two
  near-copies in `RecordUi` and `EguiUi`.

The recommended split is therefore:

- `UiContext` is a concrete type with an ergonomic, deliberately bounded API.
- `UiBackend` is the small object-safe trait with host operations.
- `UiContext` owns layout, ID stack, widget composition, and action ordering.
- `UiBackend` owns available space, allocating/reserving rects, interaction
  querying, painting, input access, cursor requests, repaint requests, and
  action emission.

This is future-proof because new widgets and layout helpers go into
`UiContext` once and automatically work with both backends. New host
capabilities go into `UiBackend` behind explicit types.

`UiBackend` is a low-level implementation detail. The SDK exposes `UiContext`
and the shared widgets; the backend is `#[doc(hidden)]`-level API for
`notwig-ui` and the extension runtime.

## Layer 1: `Drawer`

`Drawer` is a small primitive painter, deliberately close to the egui
`Painter` subset we already use.

```rust
pub trait Drawer {
    fn push_clip(&mut self, rect: Rect);
    fn pop_clip(&mut self);

    fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color);
    fn stroke_rect(&mut self, rect: Rect, radius: f32, stroke: Stroke, kind: StrokeKind);
    fn line(&mut self, from: Point, to: Point, stroke: Stroke);
    fn circle_filled(&mut self, center: Point, radius: f32, color: Color);
    fn circle_stroke(&mut self, center: Point, radius: f32, stroke: Stroke);
    fn text(&mut self, pos: Point, text: &str, font: Font, color: Color, align: Align);
    fn path(
        &mut self,
        points: &[Point],
        closed: bool,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    );
}
```

Two additions to the current IR are needed for parity:

- `Font { family: FontFamily, size: f32 }` so extensions can ask for
  proportional vs monospace text instead of always monospace.
- `Align { x: TextAlign, y: VerticalAlign }` so text can be anchored like
  egui's `Align2`, not only left/center/right at the top edge.

Implementations:

- `notwig-ext-api/src/ui/record.rs`: `RecordDrawer` implements `Drawer` by
  pushing `DrawCommand`s and maintaining a clip stack. This is the buffered
  drawer. It is WASM-safe and has no egui dependency.
- `notwig-ui/src/views/device_editor/drawer.rs`: `EguiDrawer` implements
  `Drawer` with an `egui::Painter`, a target-local origin, and a clip stack.
  This replaces `paint_draw` in `extension.rs`. It is the only place that maps
  neutral primitives to egui shapes.

Coordinates stay target-local pixels, with `(0, 0)` at the top-left of the
device panel body. `EguiDrawer` adds the target origin and intersects each clip
with the target rect. `RecordDrawer` records local coordinates unchanged.

`Drawer` is deliberately not the public interaction API. It is what
`UiContext` and shared widgets paint with.

## Layer 2: `UiContext`

`UiContext` is the egui-like API extension authors use. It is a concrete type
so it can support closures and generic IDs without object-safety problems. It
is a per-panel UI surface, not an app-wide `egui::Context`: it does not own
windows, areas, native menus, or the global paint pass.

```rust
pub struct UiContext<'a> {
    backend: &'a mut dyn UiBackend,
    layout: LayoutState,
    id_stack: Vec<UiId>,
    style: Style,
}

impl<'a> UiContext<'a> {
    // Space and layout
    pub fn available_rect(&self) -> Rect;
    pub fn allocate(&mut self, size: Size) -> Rect;
    pub fn horizontal(&mut self, add: impl FnOnce(&mut UiContext<'_>));
    pub fn vertical(&mut self, add: impl FnOnce(&mut UiContext<'_>));

    // Drawing
    pub fn painter(&mut self) -> &mut dyn Drawer;
    pub fn clip_rect(&self) -> Rect;
    pub fn set_clip_rect(&mut self, rect: Rect);

    // Interaction
    pub fn interact(&mut self, id: UiId, rect: Rect, sense: Sense) -> Response;
    pub fn input(&self) -> &UiInput;
    pub fn id(&self, source: impl Hash) -> UiId;
    pub fn push_id(&mut self, id: UiId);
    pub fn pop_id(&mut self);

    // Host reactions
    pub fn set_param_normalized(&mut self, param_id: ParamId, norm: f32);
    pub fn set_param_value(&mut self, param_id: ParamId, value: ParamValue);
    pub fn set_cursor(&mut self, cursor: CursorIcon);
    pub fn request_repaint(&mut self);

    // Style
    pub fn style(&self) -> &Style;
    pub fn theme(&self) -> ThemeSnapshot;
}
```

`Style` is the neutral subset of egui's style: spacing, widget sizes, text
sizes, and theme colors. Small neutral enums such as `CursorIcon`,
`FontFamily`, and `Sense` live in the same module.

The shape mirrors egui's `Ui` for the operations that make sense in a device
panel. It is not a re-export of egui, and it does not try to cover windows,
menus, popups, native text IME, or arbitrary host callbacks.

`UiContext` owns:

- the layout cursor and layout stack;
- the ID stack;
- widget composition and action ordering;
- the `Drawer` accessor.

The backend owns:

- available space and rect reservation;
- the interaction state machine (or delegation to egui's);
- input access;
- drawing implementation;
- cursor/repaint requests;
- action emission.

### `UiBackend`

```rust
pub trait UiBackend {
    fn available_rect(&self) -> Rect;
    fn reserve(&mut self, rect: Rect);
    fn interact(&mut self, id: UiId, rect: Rect, clip: Rect, sense: Sense) -> Response;
    fn painter(&mut self) -> &mut dyn Drawer;
    fn input(&self) -> &UiInput;
    fn style(&self) -> &Style;
    fn set_cursor(&mut self, cursor: CursorIcon);
    fn request_repaint(&mut self);
    fn emit(&mut self, action: PanelAction);
}
```

`EguiUi` maps these to `egui::Ui`/`egui::Painter`:

- `reserve` reserves space for a rect in the current egui layout.
- `interact` delegates to `ui.interact` with a namespaced `egui::Id`.
- `set_cursor` uses `ctx.set_cursor_icon`.
- `request_repaint` uses `ctx.request_repaint`.
- `emit` appends to an action buffer that `NotwigUi` lowers into `UiIntent`s.
- `painter` returns an `EguiDrawer`.

`RecordUi` implements the same trait for extensions:

- `reserve` advances the recorder's layout cursor.
- `interact` runs the neutral interaction state machine against `UiInput`,
  `UiMemory`, and the current clip.
- `set_cursor`/`request_repaint`/`emit` append `PanelAction`s.
- `painter` returns a `RecordDrawer`.

## Layer 3: Interaction model

Interaction is the main reason `UiContext` is needed. The neutral backend must
reproduce enough of egui's immediate-mode behavior that shared widgets and
buttons feel identical in both environments.

### Input snapshot

The host sends a bounded snapshot each frame. Coordinates are target-local.

```rust
pub struct UiInput {
    pub pointer: PointerState,
    pub modifiers: Modifiers,
    pub scroll: Point,
    pub time: f64,
    pub focused: bool,
    pub events: Vec<UiEvent>, // bounded; reserved for keys/text/focus later
}

pub struct PointerState {
    pub pos: Option<Point>,
    pub delta: Point,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub secondary_down: bool,
    pub secondary_pressed: bool,
    pub secondary_released: bool,
}
```

`UiEvent` is a future extension point for `Key`, `Text`, `Focus`, and
`Scroll`. Pointer-only widgets do not need it. Keeping a bounded event list in
the model now avoids a second interaction model later.

`NotwigUi` builds `UiInput` from `egui::InputState` in
`draw_extension_device_panel`: pointer position relative to the panel target,
pointer delta, primary/secondary transitions, modifiers, scroll, time, and
panel focus. Multiple raw events in one frame are aggregated for pointer
state; the bounded event list can preserve finer-grained events when keyboard
support arrives.

### Interaction memory

The neutral backend keeps per-device interaction state across frames. The SDK
owns this memory per device instance; the extension only owns its own view
state. Two devices of the same type therefore never share hover/drag state.

```rust
pub struct UiMemory {
    hovered: Option<UiId>,
    active: Option<UiId>,
    focus: Option<UiId>,
    drag: Option<DragState>,
    last_click: Option<ClickRecord>,
    hit_rects: Vec<HitRect>, // previous frame, paint order
}

struct HitRect {
    id: UiId,
    rect: Rect,
    sense: Sense,
    clip: Rect,
}

struct DragState {
    id: UiId,
    press_pos: Point,
    last_pos: Point,
    dragging: bool,
}

struct ClickRecord {
    id: UiId,
    time: f64,
    pos: Point,
}
```

`EguiUi` does not need `UiMemory`; egui already provides this behavior. It only
maps egui `Response` into the neutral `Response`.

### Hit testing and z-order

Hover cannot be computed correctly in a single forward pass when widgets can
overlap. The neutral backend uses the previous frame's hit rects, like egui:

1. At the start of a frame, compute `hovered` by scanning the previous frame's
   `hit_rects` from last to first (last painted is topmost).
2. A rect only counts if the pointer is inside its clip and it has hover or
   click/drag sense.
3. `interact(id, rect, sense)` registers the current rect for next frame and
   returns a `Response` based on the precomputed hover state and the current
   input.
4. Focus/active capture overrides hover for other widgets.

A widget that appears for the first time is not hoverable until the next frame.
That matches egui's first-frame behavior and is acceptable for panels.

### Response and state machine

```rust
pub struct Response {
    pub hovered: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub secondary_clicked: bool,
    pub dragged: bool,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub drag_delta: Point,
    pub contains_pointer: bool,
}
```

State transitions:

- **Press**: if a widget with click/drag sense is hovered, set `active` to it,
  record press position/time, and set focus. A press outside any widget clears
  active/focus.
- **Drag**: while `active` is this widget and the pointer moves beyond a small
  slop threshold, mark `dragging`. `drag_started` is true on that frame.
- **Release**: if `active` is this widget, clear it. `clicked` is true when the
  release is inside the rect and no drag happened. `drag_stopped` is true when
  a drag ends.
- **Double-click**: a release within the double-click time and distance of the
  previous click on the same ID sets `double_clicked`.
- **Hover**: true when this widget is the computed hovered widget and no other
  widget is active. `contains_pointer` is true even while another widget is
  active, for custom feedback.
- **drag_delta**: pointer movement since the previous frame while this widget
  is active.

Senses:

```rust
pub struct Sense { pub click: bool, pub drag: bool, pub hover: bool }
```

Construction helpers: `Sense::hover()`, `Sense::click()`, `Sense::drag()`,
`Sense::click_and_drag()`.

This state machine is intentionally small but covers the interactions device
panels need: buttons, toggles, knobs, sliders, XY pads, meters, and hover
feedback.

### Immediate-mode buttons and on-click handling

Because the extension is ordinary Rust code running inside WASM, it does not
need a callback ABI. It reacts in the same frame, exactly like egui:

```rust
if ui.button("Reset").clicked() {
    self.threshold_norm = self.threshold_default;
    ui.set_param_normalized(threshold_id(), self.threshold_norm);
}
```

A shared button is:

```rust
pub fn button(ui: &mut UiContext<'_>, text: &str) -> Response {
    let id = ui.id(("button", text));
    let rect = ui.allocate(Size::new(72.0, 22.0));
    let response = ui.interact(id, rect, Sense::click());
    paint_button(ui.painter(), ui.theme(), rect, &response, text);
    if response.hovered() {
        ui.set_cursor(CursorIcon::PointingHand);
    }
    response
}
```

The same function is used by built-in device panels and extension panels. The
only backend difference is how `interact` computes the `Response`.

### Actions

Actions are the only way an extension affects host state. They are validated
after the frame and are not immediate mutations.

```rust
pub enum PanelAction {
    SetParamNormalized { param_id: ParamId, norm: f32 },
    SetParamValue { param_id: ParamId, value: ParamValue },
    SetCursor { cursor: CursorIcon },
    RequestRepaint,
}
```

Rules:

- The parameter must exist on the device spec.
- Numeric values must be finite; norms are clamped to `0..=1`.
- `SetParamValue` must match the parameter kind and is clamped/rounded to the
  spec range and step.
- The host maps normalized values through `ParamSpec::value_from_norm`.
- Unknown parameters, non-finite values, mismatched kinds, and invalid cursor
  icons are dropped and logged.
- Multiple actions for the same parameter in one frame are applied in order;
  the last one wins.
- Actions are capped (`MAX_ACTIONS`) in addition to draw command caps.
- Cursor and repaint requests are applied by the host after the frame; changes
  that only affect extension-local state happen immediately in extension code.

`PanelAction` is deliberately small. Future capabilities (notifications,
clipboard, arbitrary host commands) require explicit new variants and a
capability decision.

### Focus, keyboard, and text

Pointer interaction is v1. The model leaves room for focus and keyboard:

- `UiMemory::focus` exists now; clicking a focusable widget sets it.
- `UiInput::events` is empty in v1; `Key`/`Text`/`Focus` variants can be added
  later without changing the pointer state.
- Tab order can be derived from registration order or explicit IDs.
- Text entry needs host-owned IME and focus handling. Do not expose it until
  there is a deliberate capability, a focus model, and a host-owned text field
  widget. Extensions should not implement their own IME.

Host-owned `PanelCommand::Knob`/`Toggle` can remain as a guaranteed native
interaction path. Shared immediate-mode widgets are the general path.

## Layer 4: shared widgets

Widgets are split into pure paint functions and interaction wrappers built on
`UiContext`. This is where the current duplication disappears.

```rust
pub fn paint_knob(
    drawer: &mut dyn Drawer,
    theme: &ThemeSnapshot,
    rect: Rect,
    value_norm: f32,
    bipolar: bool,
);

pub fn knob(
    ui: &mut UiContext<'_>,
    id: UiId,
    rect: Rect,
    value_norm: f32,
    default_norm: f32,
    bipolar: bool,
    label: &str,
) -> Option<f32> {
    let response = ui.interact(id, rect, Sense::click_and_drag());
    let mut norm = value_norm;
    if response.dragged() {
        let fine = ui.input().modifiers.shift;
        let speed = if fine { 0.001 } else { 0.005 };
        norm = (norm - response.drag_delta.y * speed).clamp(0.0, 1.0);
    }
    if response.double_clicked() {
        norm = default_norm;
    }
    if response.hovered() || response.dragged() {
        ui.set_cursor(CursorIcon::ResizeVertical);
    }
    let theme = ui.theme();
    paint_knob(ui.painter(), &theme, rect, norm, bipolar);
    paint_label(ui.painter(), &theme, rect, label);
    (norm != value_norm).then_some(norm)
}
```

What to share:

- `paint_knob`, `paint_toggle`, `paint_button`, `paint_label`,
  `paint_section`, `paint_divider`, and a stereo meter painter move into
  `notwig-ext-api/src/ui/widgets.rs`.
- `notwig-ui/src/widgets/knob.rs` keeps the egui-specific numeric text editor
  and calls the shared painter through `EguiDrawer`. The duplicated knob in
  `views/device_editor/mod.rs` is deleted.
- Theme constants in `notwig-ui/src/widgets/theme.rs` become a conversion to
  `ThemeSnapshot`; extension code receives the snapshot in its context.

Text entry (the current Cmd/Ctrl+click numeric editor in
`widgets::value_drag`) is host-specific initially. Extensions get drag, fine
drag, double-click reset, and hover feedback until a text capability exists.

## Frame contract

A frame is a transient command list plus actions. Command order defines paint
order.

```rust
pub struct PanelDraw {
    pub commands: Vec<PanelCommand>,
    #[serde(default)]
    pub actions: Vec<PanelAction>,
}
```

`PanelCommand::Draw(DrawCommand)` covers `UiContext` painting. The existing
host-owned `PanelCommand::Knob`/`Toggle` can remain as a fast path and as a
guaranteed native interaction option. Both paths converge on the same
`ParamSpec` validation.

## Host-side integration

`notwig-ui/src/views/device_editor/extension.rs` becomes:

1. allocate the panel target;
2. build target-local `UiInput` from egui input;
3. build `DeviceDrawContext` with params, size, theme, and input;
4. call the panel host for the **device instance**;
5. validate `PanelDraw`;
6. replay draw commands through `EguiDrawer`;
7. apply cursor/repaint actions immediately;
8. lower parameter actions into `UiIntent::SetParam`.

`ExtensionPanelHost` in `crates/notwig-ext/src/panel.rs` must key UI instances
by `device_id`, not `type_id`. A panel instance is per device, and its
`UiMemory` (hover/active/focus/drag) is per device too. Two limiters then
interact independently.

## Extension-side integration

1. Change the SDK trait from static panel data to a `UiContext` draw:

```rust
pub trait DeviceExtension: Send + Sized + 'static {
    fn new() -> Self;
    fn spec() -> DeviceSpec;
    fn draw(&mut self, ui: &mut UiContext<'_>);
    fn prepare(&mut self, sample_rate: u32, channels: u16) {}
    fn set_param(&mut self, param_id: ParamId, value: f64) -> bool { false }
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32 { 0 }
}
```

The SDK `DeviceSlot` owns the `UiMemory`. The extension instance owns only its
own view state (for example cached norm values or custom drag modes).

2. `Extension::draw_pointer(handle, context_json)` deserializes the context,
   constructs `RecordUi` with the slot's `UiMemory`, calls
   `DeviceExtension::draw`, serializes the resulting `PanelDraw`, and returns a
   stable pointer to the last output buffer. This mirrors the existing
   `panel_pointer` cached-output pattern.

3. `#[notwig_extension]` must read the context bytes instead of ignoring them:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn notwig_device_draw(
    handle: i32,
    ctx_ptr: i32,
    ctx_len: i32,
) -> i64 {
    with_bytes(ctx_ptr, ctx_len, |ctx| {
        extension_state().draw_pointer(handle, ctx)
    })
    .unwrap_or(0)
}
```

`with_bytes` is a small addition next to the existing `with_str` in
`notwig-ext-api/src/abi/pointers.rs`.

4. Update `notwig-ext-devices` to use the new `draw` method. The limiter can
   start as a static layout built in `draw`, then gain hover/drag interaction.

## ABI and compatibility

The `notwig_device_draw` signature does not need to change: the context blob
grows with input, and the returned blob grows with actions. Add serde defaults
so a missing `actions` field is an empty list.

The Rust trait change is not source-compatible (static `panel()` becomes
`draw()`). Because extensions are new and experimental:

- bump `ABI_VERSION` to `2` in `notwig-ext-api/src/abi.rs`;
- update the extension manifest `api_version` check;
- rebuild the bundled limiter extension.

If keeping old binaries matters more than simplicity, the host can continue to
accept ABI 1 by caching its static panel. That compatibility branch should be
explicit, not accidental.

## Layout, IDs, and text

- `UiContext` owns the layout cursor. The backend only reserves rects. This
  keeps `EguiUi` and `RecordUi` aligned and avoids relying on egui's internal
  cursor for neutral widgets.
- Start with `horizontal`, `vertical`, `allocate`, spacing, and fixed widget
  sizes. This covers device panels without a full layout engine.
- Add `row`/`column`/spacing/alignment helpers before adding complex widgets.
- Text measurement is not free: the extension does not share egui's font atlas.
  For v1, layout uses fixed control sizes and text is placed inside allocated
  rects; the host performs true glyph layout. If dynamic text width becomes
  necessary, add a small `TextMetrics` snapshot to the context (line height,
  advance per character for proportional and monospace fonts) rather than a
  per-call host import.
- `UiId` is explicit: either a raw `u64` or a stable hash of a string. IDs are
  chosen by the extension, not allocated by declaration order, so interaction
  state survives frames.
- ID stacks namespace repeated widgets. `ui.id("threshold")` extends the current
  stack; `push_id`/`pop_id` support nested groups.

## Security and limits

The trust model does not change: extensions still return data, not pointers
into host UI. Interaction runs inside the extension's own memory. Additional
validation:

- cap actions (for example `MAX_ACTIONS = 256`) as well as draw commands;
- reject non-finite norms, unknown parameter IDs, and actions for parameters
  not present in the device spec;
- keep text, path, command, and output byte caps;
- keep the per-draw fuel/time budget for `Limited` and `DrawOnly` extensions;
- keep `DrawOnly` devices silent in DSP while still drawing;
- rate-limit or coalesce `RequestRepaint` actions so an extension cannot spin
  the UI thread;
- restrict `SetCursor` to a known cursor icon enum;
- do not expose clipboard, file, network, keyboard text input, or host
  callbacks until there is a deliberate capability and ABI for them.

Input snapshots are small, host-generated values. They leak only pointer and
modifier state inside the target panel, which is already observable by the
extension through user interaction.

## Performance

The design serializes JSON per frame. Input snapshots make the context blob
larger, so this is the main cost once panels are dynamic:

- `DeviceDrawContext` now contains params, theme, input, and optionally events;
  `PanelDraw` contains commands and actions.
- Keep JSON for debuggability in v1. Measure before changing the wire format.
- If profiling shows a problem, switch the per-frame blobs to a compact binary
  encoding (for example `postcard`) while keeping manifests and `DeviceSpec`
  JSON. Do this as a wire-version change, not by overloading JSON fields.
- Keep the event list bounded and only include events that the UI layer needs.
- Immediate mode means redraw-on-demand, not necessarily redraw-everything when
  nothing changed. Extensions can skip expensive internal work when the input
  snapshot and params are unchanged.

## Testing

- `RecordDrawer` tests: each primitive produces the expected command; clip
  push/pop is balanced and clipped.
- `EguiDrawer` tests: run a headless `egui::Context`, paint through the drawer,
  and inspect the produced shapes/clip rects.
- Interaction state machine tests on `RecordUi`:
  - press and release inside a widget -> `clicked`;
  - press inside, release outside -> not clicked;
  - press, move beyond slop, release -> `dragged` then `drag_stopped`, not
    clicked;
  - double-click within time/distance -> `double_clicked`;
  - overlapping widgets -> topmost painted widget wins hover;
  - clip rect prevents hover outside the clip;
  - pointer capture keeps the active widget receiving `drag_delta`;
  - two devices of the same type keep independent memory.
- Shared widget parity tests: run the same widget code against `EguiUi` and
  `RecordUi` with identical `UiInput`; assert the same visual commands and the
  same parameter action (for example a drag changes the knob by the expected
  norm).
- Immediate-mode button test: an extension button click changes extension-local
  state and emits the expected action in the same frame.
- Action validation tests: out-of-spec parameters, non-finite values, invalid
  cursors, and oversized action lists are rejected.
- ABI tests: a WAT fixture returns a dynamic panel from a context; input and
  actions round-trip; invalid output is rejected.
- UI tests: headless render of a built-in device and an extension panel with the
  same theme, asserting they use the same primitive output.

## Migration plan

### Phase 1: extract the neutral painter

1. Add `notwig-ext-api/src/ui/drawer.rs` with `Drawer`, `Font`, `Align`, and
   `StrokeKind`.
2. Add `RecordDrawer` and make `DrawList`/`DrawCommand` its output.
3. Add `notwig-ui/src/views/device_editor/drawer.rs` with `EguiDrawer`.
4. Replace `paint_draw` in `extension.rs` with `EguiDrawer`.
5. Move pure paint bodies for knob, toggle, button, label, section, divider,
   and meter into `notwig-ext-api/src/ui/widgets.rs`; make `notwig-ui` call
   them.
6. Add drawer and shared-widget tests.

### Phase 2: dynamic draw and per-device UI state

1. Change `DeviceExtension::panel()` to `draw(&mut UiContext<'_>)`.
2. Make the macro read the context and call `Extension::draw_pointer`.
3. Key `ExtensionPanelHost` instances by `device_id`.
4. Add `PanelDraw.actions` and action validation/lowering.
5. Add `UiContext`/`UiBackend` with the first backend methods needed for layout
   and static widgets.
6. Update the limiter example and its tests.

### Phase 3: immediate-mode interaction

1. Add `UiInput`, `Response`, `Sense`, `UiId`, `UiMemory`, `DragState`, and
   `ClickRecord`.
2. Implement `RecordUi`'s hit testing and interaction state machine.
3. Implement `EguiUi` and map egui responses/cursors/repaint.
4. Implement shared `button`, `toggle`, and knob interaction.
5. Build `UiInput` from egui input in `draw_extension_device_panel`.
6. Add interaction and backend-parity tests.
7. Extend `DeviceDrawContext` with `UiInput`.

### Phase 4: layout and richer widget set

1. Add `horizontal`/`vertical`/`columns` and spacing/alignment helpers.
2. Add sliders, value drag, combo boxes, meters, and generic buttons as typed
   shared widgets.
3. Add focus, Tab order, and keyboard events without text IME.
4. Add a text input capability only with a deliberate capability flag and
   host-owned focus/IME.
5. Revisit general arranger/clip/track drawing hooks using the same
   `UiContext`/`UiBackend` split.

## Open questions

- Should `notwig-ext-api` remain the home of the neutral UI layer, or should a
  `notwig-ui-primitives` crate be extracted if non-extension consumers appear?
  For now `notwig-ext-api` is the lowest crate that both the WASM SDK and
  `notwig-ui` already share, so it is the smallest correct move.
- Do we want to keep `PanelCommand::Knob`/`Toggle` forever, or deprecate them
  once shared immediate-mode widgets emit actions? Keeping them is useful as a
  guaranteed host-owned interaction path.
- Should the wire format stay JSON per frame, or move to a compact binary once
  profiles justify it?
- Should pointer input remain an aggregated per-frame snapshot, or should the
  event list become the primary interaction source for parity with egui's raw
  event handling?
- How much keyboard support (focus, Tab order, text entry) is required before
  extension panels feel complete? Start with pointer-only widgets.
- Should repaint requests be unconditional, or coalesced to one per frame with
  a budget?
