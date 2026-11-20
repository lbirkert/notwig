# Extensions

Status: first pass implemented. The shared SDK (`notwig-ext-api`), wasmtime host
(`notwig-ext`), extensible device panels, WASM DSP devices, trust levels, and
the Settings > Extensions registry/browser are in place. Arranger/clip/track
drawing hooks remain future work.

## Goal

Add a first-class extension system to Notwig focused first on **extensible
devices**:

1. **Devices**: extensions supply metadata, DSP, and a custom device panel.
2. **Drawing**: extension device panels can draw Notwig/egui-like primitives and
   Notwig widgets; general arranger/clip hooks come later.

Extensions are compiled to WebAssembly and loaded by a host runtime inside
`notwig-app`. Extensions never mutate `ProjectDoc` directly; they observe
host-provided snapshots and emit draw commands or intents that the app
validates.

## Resolved decisions

| Topic | Decision |
|---|---|
| Runtime | `wasmtime` with Cranelift JIT/AOT |
| First pass | Phased: shared draw IR + device abstraction, then WASM devices + panel rendering |
| ABI | Rust SDK, core wasm, versioned exports, JSON metadata/snapshots, raw `f32` audio |
| Device UI | Native built-in renderers + extension device panels through a host UI API; generic params as fallback |
| Discovery | User extensions directory only, backed by an `ExtensionRegistry` and an extension browser |
| Hook surface | Device panels first; arranger/clip/track hooks later |
| DSP limits | Per-extension trust level: trusted by default, selectable stricter sandboxing |

## Is WASM viable for DSP?

Yes, with constraints. WASM is viable for user-facing effects and simple
instruments, but it is not a free replacement for native DSP.

What works:

- **Block processing.** Call a single `process` export per engine block and pass
  interleaved `f32` samples through linear memory. This keeps boundary overhead
  low and matches the existing `DeviceProcessor::process` contract.
- **JIT/AOT execution.** `wasmtime` with Cranelift produces near-native scalar
  code. Modules are compiled once at scan/load time and can optionally be cached
  as AOT `.cwasm` files, so the audio callback never compiles or instantiates.
- **No allocation in the callback.** Buffers, note-event arrays, and parameter
  slots are allocated in `prepare` or earlier. The process call only reads/writes
  prepared regions.
- **Deterministic sandbox.** Core wasm without WASI has no filesystem, network,
  threads, or clocks.

What does not work well yet:

- **Very low latency / very high instance counts.** Sandbox bounds checks,
  indirect call overhead, and per-instance memory make WASM heavier than native
  for tiny blocks and hundreds of instances. Acceptable for extension devices,
  not for the core mixer path.
- **Heavy SIMD synthesis.** `simd128` exists but is not universal and not
  ergonomic through a stable ABI. Scalar `f32` is the baseline.
- **Real-time safety is trust-based.** An untrusted extension can run a long
  loop and starve the callback, just like a native plugin. Fuel can preempt it at
  a measurable cost; trusted extensions skip that overhead.
- **Threading and shared state.** WASM threads are out of scope. Devices keep
  their state inside their own instance store.

Recommendation: `wasmtime`, block processing, precompiled modules, a small DSP
ABI, and user-selectable trust levels.

## Can we pass the egui context to WASM?

No, not directly. `egui::Context` is a Rust type with lifetimes, thread-bound
state, and no stable FFI layout. Passing it through a wasm boundary would be
unsafe, non-portable, and would let extensions drive arbitrary host state.

What we can do instead is give extensions an **egui-like drawing and widget API
through serialized panel commands**:

- The extension builds a `PanelDraw` with `notwig-ext-api` types and returns it
  from `notwig_device_draw`. The command set mirrors the egui painter:
  rects, strokes, lines, circles, paths, text, and clip push/pop.
- Higher-level panel commands cover consistent Notwig controls:
  `PanelCommand::Knob`, `Toggle`, `Label`, and `Section`.
- The host validates the command list and replays it into the real
  `egui::Painter`/`egui::Ui`. Knobs and toggles are rendered by native Notwig
  widgets, so interaction and parameter intents stay host-owned.
- Extensions therefore never receive or mutate `egui::Context`, `Ui`, or host
  pointers. Only the validated command list crosses the boundary.

This is not arbitrary egui access: no custom `Widget` implementations, no
`Ui::add`, no layout callbacks. That limitation is intentional. If a future
extension needs a new widget or layout primitive, it should be added as a typed
`PanelCommand` or host import so it stays themed, accessible, and safe.

The v1 device-panel API is:

- **Layer 1: primitives.** rects, strokes, lines, circles, paths, text, clip
  regions.
- **Layer 2: Notwig widgets.** parameter knob with a `ParamId`, toggle, label,
  section header. These map directly to `notwig-ui` widgets and emit intents;
  extensions cannot invent parameter values the host does not know about.
- **Layer 3 (later): layout.** a constrained vertical/horizontal layout stack
  with spacing/alignment, so panels can be composed without pixel math.

## Extension package and registry

A directory per extension under the per-user Notwig extensions directory:

```text
<id>/
  extension.json     manifest
  extension.wasm     core wasm module (wasm32-unknown-unknown)
  assets/            optional extension-owned assets
```

`extension.json`:

```json
{
  "id": "com.example.spectrum",
  "name": "Spectrum",
  "vendor": "Example",
  "version": "0.1.0",
  "api_version": 1,
  "entrypoint": "extension.wasm",
  "capabilities": ["device", "ui.draw"],
  "devices": [{ "name": "Spectrum", "type_id": "com.example.spectrum.fx", "version": 0 }],
  "description": "A spectrum analyzer and filter.",
  "image": "icon.png"
}
```

Rules:

- `id` is the stable package identity; exported type IDs should use it as a
  prefix.
- `api_version` is checked against the host. Unknown major versions are
  rejected; missing extensions leave devices unresolved but do not fail project
  load.
- `capabilities` gate host imports: `device` for DSP, `ui.draw` for panels.
- Discovery only scans the user extensions directory. Project-local extensions
  are deliberately out of scope until there is a trust/opt-in flow.

`ExtensionRegistry` (app-level) owns:

- installed extension metadata and manifest;
- load status (`Discovered`, `Loaded`, `Disabled`, `Failed(reason)`);
- per-extension **trust level** and enabled flag, persisted in user config;
- the compiled wasm module handle;
- the device specs and renderer hooks it contributes.

The extension browser is a Settings page (and later a device-browser source)
listing name, vendor, version, capabilities, status, enable toggle, trust level,
and a reload action. It reads the registry; it does not scan the filesystem on
the UI thread.

## Crate layout

```text
notwig-ext-api      (new, light)
  author-facing extension SDK plus shared boundary types:
  draw/UI command IR, ABI helpers, DeviceExtension/Extension, manifests
  depends on: notwig-core, notwig-dsp, serde

notwig-ext-macros   (new, proc-macro)
  #[notwig_extension] generates the wasm exports around the entrypoint
  re-exported by notwig-ext-api

notwig-ext   (new, heavy)
  discovery, wasmtime engine/module cache, store lifetimes,
  host imports, WASM device processor, UI host runtime
  depends on: notwig-core, notwig-dsp, notwig-ext-api, wasmtime

notwig-dsp
  DeviceSpec + DeviceUiSpec, DeviceProcessor, DeviceFactory, CompiledPlan
  no egui, no wasmtime

notwig-ui
  interprets draw/UI commands with egui; dispatches device renderers by spec
  depends on: notwig-core, notwig-project, notwig-dsp, notwig-ext-api

notwig-app
  owns ExtensionRegistry + ExtensionRuntime, composes device factories,
  hosts the extension browser, sets the UI host
```

Dependency direction stays acyclic. `notwig-ui` depends on the command IR only,
not on the runtime; `notwig-ext` never depends on UI or app.

## Device abstraction

Today three concerns are mixed:

1. metadata (`DeviceSpec`),
2. DSP (`DeviceProcessor` + `instantiate_stock*` + `DeviceFactory`),
3. presentation (hard-coded `type_id` checks in `notwig-ui`).

Split them.

### Spec and UI hint

Add a serialized-with-default UI hint to `DeviceSpec`:

```rust
#[derive(Default, Serialize, Deserialize)]
pub enum DeviceUiSpec {
    #[default]
    Generic,
    Custom {
        renderer: String,          // stable renderer id
        preferred_width: Option<f32>,
    },
}
```

Stock specs declare `notwig.eq`, `notwig.tool`, `notwig.chain`. Plugin specs use
`Generic`. Extension specs use `Custom { renderer: "ext:<type_id>" }` and render
through the extension UI host. `notwig-ui` maps built-in renderer ids to native
egui functions and falls back to the generic parameter editor. Card width comes
from the spec, not from matching type IDs.

### Device factory

Move the `DeviceFactory` trait from `notwig-engine` into `notwig-dsp` so every
device source implements the same interface:

```rust
pub trait DeviceFactory: Send + Sync {
    fn create(&self, plan: &CompiledPlan, device: &PlanDevice)
        -> Option<Box<dyn DeviceProcessor>>;
}
```

Add `StockDeviceFactory` and `CompositeDeviceFactory` to `notwig-dsp`.
`PluginDeviceFactory` keeps its plugin-host implementation. The extension crate
provides `ExtensionDeviceFactory`. `notwig-app` composes:

```text
CompositeDeviceFactory[
  StockDeviceFactory,
  ExtensionDeviceFactory,
  PluginDeviceFactory,
]
```

Engine code continues to consume one `Arc<dyn DeviceFactory>`.

### Registration

- Extension device specs are registered in the same `DeviceRegistry` used by
  `compile`.
- The device browser lists every registry spec; insert uses a single generic
  `InsertDevice` intent rather than stock-vs-plugin branches.
- Missing extensions leave `PlanDevice::resolved = false`, matching today's
  missing-plugin behavior.

## Extension ABI (v1)

Core wasm, no WASI. The SDK is a Rust crate; the wire format is JSON for
metadata/snapshots and raw `f32` for audio.

Required exports:

```text
notwig_abi_version() -> i32
notwig_alloc(len: i32) -> i32
notwig_free(ptr: i32, len: i32)

notwig_device_describe(type_id_ptr: i32, len: i32) -> i64
                                   // packed ptr/len for JSON DeviceSpec
notwig_device_create(type_id_ptr: i32, len: i32) -> i32
notwig_device_destroy(handle: i32)
notwig_device_prepare(handle: i32, sample_rate: i32,
                      max_block: i32, channels: i32) -> i32
notwig_device_set_param(handle: i32, param_ptr: i32, param_len: i32,
                        tag: i32, value: f64) -> i32
notwig_device_process(handle: i32, audio_ptr: i32, frames: i32,
                      events_ptr: i32, event_count: i32,
                      sample_position: i64, playing: i32) -> i32
notwig_device_latency(handle: i32) -> i32
notwig_device_draw(handle: i32, ctx_ptr: i32, ctx_len: i32) -> i64
                                   // packed ptr/len for JSON PanelDraw
```

General host imports:

```text
notwig.host.log(level, ptr, len)
notwig.host.notify(ptr, len)        // user-visible status
```

`notwig_device_draw` receives the serialized `DeviceDrawContext` and returns a
serialized `PanelDraw`. There are no per-frame `notwig.ui.*` imports in v1; the
SDK's draw-command builder is the egui-like API. Layout/widget host imports can
be added later without changing device IDs.

The host calls `notwig_alloc` for inputs and reads outputs from memory the
extension already owns; all pointers are checked against the module's memory
bounds. Extensions cannot allocate host memory or access host pointers.

## Runtime lifecycle and threading

- **Discovery/scan** runs off the UI thread at startup and on reload. It reads
  manifests, validates ABI versions, compiles modules once, and caches compiled
  modules.
- **DSP instances** are created by `ExtensionDeviceFactory` during engine build
  on the build thread. A `WasmDeviceProcessor` owns one `wasmtime::Store` and one
  instance. `process` calls a typed export with prepared buffers and performs no
  allocation.
- **UI instances** for device panels are separate stores owned by the UI-side
  host and reused across frames. They never share mutable state with DSP
  instances.
- **Parameters/state** cross the boundary as scalar values. Device state that is
  not represented as parameters is extension-owned; persistence is deferred
  until a state blob ABI exists.
- **Latency** is the static `DeviceSpec::latency_samples` plus an optional
  dynamic query after `prepare`.

## Trust levels and limits

Every extension has a persisted `TrustLevel`; the extension browser exposes it.

| Level | UI | DSP | Limits |
|---|---|---|---|
| `Trusted` (default) | Yes | Yes | 64 MiB store, validated inputs/outputs, no fuel |
| `Limited` | Yes | Yes | 32 MiB store, per-block fuel budget, UI hook timeout |
| `DrawOnly` | Yes | No | 16 MiB store, fuel/time-limited UI calls |

- `DrawOnly` is for extensions whose DSP the user does not trust; their devices
  load but process silence and report an unresolved/bypassed status.
- Fuel traps are caught at the `WasmDeviceProcessor` boundary; the device
  produces silence for that block and reports `ProcessStatus::overloaded`.
  Sustained traps surface through the existing DSP timing/status path.
- All levels cap module size, output bytes, command counts, text bytes, and path
  points.
- UI hooks have a wall-clock budget per frame; slow hooks are skipped and
  eventually disabled for the session.
- Extensions cannot read project files, the network, the filesystem, or other
  extensions. They only see the snapshots the host passes.

## Drawing command IR

Extensions return a render-neutral, immediate-mode command list in target-local
pixels. The host translates it to `egui` shapes and clips it to the surface.

```rust
pub struct DrawList { pub commands: Vec<DrawCommand> }

pub enum DrawCommand {
    PushClip(Rect),
    PopClip,
    FillRect { rect: Rect, radius: f32, color: Color },
    StrokeRect { rect: Rect, radius: f32, width: f32, color: Color },
    Line { from: Point, to: Point, width: f32, color: Color },
    CircleFill { center: Point, radius: f32, color: Color },
    CircleStroke { center: Point, radius: f32, width: f32, color: Color },
    Text { pos: Point, text: String, size: f32, color: Color, align: TextAlign },
    Path { points: Vec<Point>, closed: bool, fill: Option<Color>, stroke: Option<Stroke> },
}
```

Host-side validation:

- Reject non-finite coordinates, colors, widths, and sizes.
- Clamp radii and widths to the target size.
- Cap commands, path points, text bytes, and cumulative output.
- Unknown renderer ids or missing extensions fall back to the generic parameter
  editor.
- Built-in content is drawn first; extension device panels replace the generic
  body.

## Migration plan

Each phase is independently shippable.

### Phase 1: shared draw IR and device UI dispatch (implemented)

1. Add `notwig-ext-api` with `Point`, `Rect`, `Color`, `DrawCommand`,
   `DrawList`, UI command types, draw contexts, and validation helpers.
2. Add `DeviceUiSpec` to `DeviceSpec` with serde defaults.
3. Add a `notwig-ui` renderer dispatch keyed by renderer id. Move the EQ, Tool,
   and Chain bodies out of `device_editor/mod.rs` into renderer functions.
   Remove `type_id` checks from card width/body selection.
4. Add a `DevicePanelHost` trait and a field on `NotwigUi`; built-in renderers
   use native egui, extension renderers produce draw commands.
5. Add a headless test for generic fallback and renderer dispatch.

### Phase 2: device factory composition (implemented)

1. Move `DeviceFactory` into `notwig-dsp`; add `StockDeviceFactory` and
   `CompositeDeviceFactory`.
2. Keep `PluginDeviceFactory` behavior; implement the moved trait.
3. Change `EngineBuildRequest` to accept an optional extension factory and
   compose factories in `notwig-app`.
4. Replace stock-vs-plugin insert branches with one generic device insert path.

### Phase 3: extension registry, runtime, and device panels (implemented)

1. Add `notwig-ext`: manifest parsing, user-directory discovery, wasmtime
   engine, module cache, store creation, host imports, ABI versioning.
2. Add `ExtensionRegistry` persistence (enabled + trust level) and the
   Settings > Extensions browser with status and reload.
3. Implement `notwig_device_describe` and `notwig_device_draw` with serialized
   `PanelDraw` output (including native knob/toggle commands).
4. Register extension device specs in `DeviceRegistry`; add them to the device
   browser.
5. Wire `ExtensionManager` into `NotwigApp`; install the panel host in
   `NotwigUi`.

### Phase 4: WASM devices and trust levels (implemented)

1. Implement `WasmDeviceProcessor` and `ExtensionDeviceFactory`.
2. Implement `notwig_device_create/destroy/prepare/set_param/process/latency`.
3. Enforce trust levels (memory/fuel/draw-only) and report traps.
4. Persist extension device parameters through existing `DeviceInstance`
   `param_values`; defer state blobs.

### Phase 5 (deferred): general drawing hooks

Add `arranger_clip`, track header, lane, audio editor, and MIDI editor hooks on
top of the same `DrawList`/`DevicePanelHost` infrastructure. Hooks are draw-only
until an interaction ABI exists.

## Example extension: `notwig-ext-devices`

`crates/notwig-ext-devices` is a working reference extension. It builds to
`wasm32-unknown-unknown` with `crate-type = ["cdylib", "rlib"]` and ships one
device, `notwig.ext.devices.limiter`.

Devices implement `DeviceExtension` and register from a macro entrypoint:

```rust
#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<LimiterDevice>();
}
```

The macro generates the wasm exports; the trait owns spec, panel, prepare,
parameter, process, and latency behavior.

- DSP: stereo-linked peak limiter with threshold (-24..0 dB) and release
  (0.01..0.5 s), instant attack, release-smoothed recovery, zero latency.
- Panel: `PanelDraw` with a section header, two native knobs, a divider, and a
  label.
- ABI: SDK-provided bump heap, `DeviceStore`, and pointer/audio helpers; no
  wasm imports.
- Build/install steps live in the crate README.

The wasm build depends on `notwig-core` and `notwig-dsp` with
`default-features = false` so UUID `v4`/getrandom is not pulled into the module.
`notwig-core` keeps `rand` as a default feature for native command application.

## Testing

- **ABI/WAT fixtures.** Use the `wat` crate as a dev-dependency to compile tiny
  modules inline. No wasm toolchain needed in CI.
  - device describe/draw returns a known command list;
  - invalid/oversized output is rejected;
  - gain device multiplies samples and reports latency;
  - unknown ABI/type IDs fail without panicking;
  - a fuel-limited device that hangs traps and produces silence.
- **Draw IR tests.** Coordinate clamping, non-finite rejection, command caps.
- **UI tests.** Headless `egui` render with a fake panel host; assert renderer
  dispatch, generic fallback, and clip containment.
- **DSP/engine tests.** Compile a plan with a WASM device, render audio, verify
  gain/latency; verify a missing extension resolves as `resolved = false`.
- **Workspace tests.** Insert an extension device through the generic insert
  path and undo/redo it; persist and reload the extension registry.
