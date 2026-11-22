# Can extensions get the real `egui` API?

Status: research report. Implementation is documented in `docs/03_development/12_egui_extension_panels.md`.
Scope: sandboxed WASM extensions (`wasmtime`, core wasm, `wasm32-unknown-unknown`) and the optional native dylib path (`libloading` + C ABI), under the workspace constraints (`#![forbid(unsafe_code)]`, Rust 2024, egui 0.32, macOS first, browser later).

## Verdict

**Yes, but only in a narrow, literal sense.** The practical way to give extension authors the real `egui` API is to run a *private* `egui::Context` inside the extension module, let the extension call normal egui code, tessellate there, and send to the host serialized triangle meshes, texture deltas, and platform output. The host composites those meshes into a clipped sub-surface and routes input/focus to the extension. The extension gets `ui.add`, custom `Widget` implementations, `Grid`, `ScrollArea`, `TextEdit`, `Window`, `Area`, `Painter`, and the rest of egui *inside its own context*.

That is **not** the same as passing the host's `egui::Context` or `Ui` across the boundary, and it is **not** arbitrary host UI integration. The extension cannot add widgets to host layouts, participate in host focus/accessibility trees, or float popups over host UI outside its panel without a much larger protocol.

- **Practical for a small project:** run egui in the extension's wasm and transfer meshes/textures (Option A). This preserves the wasm sandbox and needs no unsafe code in the host.
- **Not practical:** pass the host `Context`/`Ui` into a dylib or call host egui methods directly (Option B). Rust has no stable ABI for these types, egui's context is private, its UI objects are frame-scoped, and the project forbids unsafe outside the existing native adapter.
- **Safe but not "full egui":** expand typed host imports (WIT/component model or core-wasm host calls) so extensions drive host-owned egui widgets (Option C). This gives a large, curated subset with host-owned theming and the only realistic path to accessibility, but never `impl Widget` or arbitrary `Ui` methods.

Recommended direction: keep the current typed `PanelCommand`/host-widget API as the default, add Option A as an opt-in `ui.egui` capability for wasm extension panels, and treat Option C as the path for accessible, host-integrated controls. Do not add native full-egui plugins to the main process.

## Options at a glance

| Option | What authors get | Isolation | Host work | Extension work | Browser | Effort | Main risk | Verdict |
|---|---|---|---|---|---|---|---|---|
| D. Status quo draw IR | `UiContext`/`Drawer`/`PanelCommand` facade | WASM sandbox; validated commands | Render commands into egui | Learn Notwig API | Yes | None | Limited API | Keep as default |
| A. Guest `egui::Context` + tessellated meshes | Real egui API inside a panel | WASM sandbox; host must validate raw meshes/textures | Input/focus/IME bridge, texture namespace, mesh validation, compositor | Link egui, implement UI, serialize frame | Yes (core wasm, JS glue) | Medium-large | Raw-mesh validation surface, font atlas transfer, a11y gap | Best way to get real egui; opt-in |
| B. Host `Context`/`Ui` into native dylib | Real egui API on host state | None; in-process UB/crash | Unsafe pointer plumbing | Must match exact host egui build | No | Small prototype, huge risk | ABI/version skew, unsafe, crashes | Reject |
| B2. Host-proxied egui calls (C ABI handles) | A remote-control subset of egui | WASM sandbox or native | Every egui method as a host function; `Ui` stack | Use generated proxies | Possible but heavy | Very large | Closure/lifetime/re-entrancy complexity; still not full | Reject vs C |
| C. Expanded typed host imports (WIT/core wasm) | Larger curated egui subset | WASM sandbox; host-validated | Implement each widget/layout/text call | Use generated bindings | Core wasm yes; WIT needs tooling | Incremental | Never reaches "full egui" | Best for host-owned UI |

## Repo baseline (what exists today)

The current panel API is deliberately a serialized command IR, not egui:

- `UiContext` is a Notwig-owned immediate-mode facade over a `UiBackend`, with explicit allocate/layout/interact/painter methods. It never exposes egui: [`crates/notwig-ext-api/src/ui/mod.rs:579-583`](../crates/notwig-ext-api/src/ui/mod.rs#L579-L583), [`crates/notwig-ext-api/src/ui/mod.rs:525-540`](../crates/notwig-ext-api/src/ui/mod.rs#L525-L540).
- Extension-side recording buffers `DrawCommand`s in `RecordDrawer`; `RecordUi` turns them into a `PanelDraw`: [`crates/notwig-ext-api/src/ui/record.rs:31-108`](../crates/notwig-ext-api/src/ui/record.rs#L31-L108), [`crates/notwig-ext-api/src/ui/mod.rs:810-843`](../crates/notwig-ext-api/src/ui/mod.rs#L810-L843).
- The wire types are `PanelCommand` and `PanelAction`, validated by `PanelDraw::validate`: [`crates/notwig-ext-api/src/panel.rs:13-51`](../crates/notwig-ext-api/src/panel.rs#L13-L51), [`crates/notwig-ext-api/src/panel.rs:83-132`](../crates/notwig-ext-api/src/panel.rs#L83-L132).
- The host replays commands through `EguiDrawer` and native widgets: [`crates/notwig-ui/src/views/device_editor/drawer.rs:11-42`](../crates/notwig-ui/src/views/device_editor/drawer.rs#L11-L42), [`crates/notwig-ui/src/views/device_editor/extension.rs:45-168`](../crates/notwig-ui/src/views/device_editor/extension.rs#L45-L168).
- The design decision is written down: no `egui::Context`/`Ui` across the boundary, only validated command lists: [`plan/EXTENSIONS.md:70-93`](EXTENSIONS.md#L70-L93), [`docs/02_extensions/04_panel_api.md:218-223`](../docs/02_extensions/04_panel_api.md#L218-L223).
- The existing ABI is core wasm, JSON for metadata/snapshots, raw `f32` audio: [`plan/EXTENSIONS.md:257-299`](EXTENSIONS.md#L257-L299), [`crates/notwig-ext-api/src/abi.rs:12`](../crates/notwig-ext-api/src/abi.rs#L12), [`crates/notwig-ext/src/abi.rs:10-12`](../crates/notwig-ext/src/abi.rs#L10-L12).
- Limits today: 4096 commands, 1024 path points, 8 KiB text, 256 KiB output, 256 actions: [`crates/notwig-ext-api/src/draw.rs:5-9`](../crates/notwig-ext-api/src/draw.rs#L5-L9). WASM store limit is 64 MiB and fuel is 20M per call: [`crates/notwig-ext/src/abi.rs:10-12`](../crates/notwig-ext/src/abi.rs#L10-L12).
- The workspace denies unsafe code globally: [`Cargo.toml:108`](../Cargo.toml#L108); UI/app/core crates `forbid` it: [`crates/notwig-ui/src/lib.rs:1`](../crates/notwig-ui/src/lib.rs#L1), [`crates/notwig-app/src/lib.rs:1`](../crates/notwig-app/src/lib.rs#L1). The native dylib adapter is the one explicit exception: [`crates/notwig-ext/src/native.rs:1`](../crates/notwig-ext/src/native.rs#L1) (`#![allow(unsafe_code)]`).
- The web build instantiates core wasm through the browser `WebAssembly` API, not `wasmtime`: [`crates/notwig-ext/src/web/host.rs:77`](../crates/notwig-ext/src/web/host.rs#L77), [`crates/notwig-ext/src/web/instance.rs:80`](../crates/notwig-ext/src/web/instance.rs#L80). `wasmtime` itself does not run on `wasm32-unknown-unknown`: [`plan/BROWSER_BUILD.md:213-215`](BROWSER_BUILD.md#L213-L215).

## Option A: run egui inside the extension, send meshes + textures

### A.1 Exactly which egui APIs support this

egui is designed to be embedded by a backend. The documented integration loop is: build `RawInput`, call `Context::run`, handle `FullOutput`, tessellate `FullOutput::shapes`, then paint `TexturesDelta` + `ClippedPrimitive`s. That is the same loop eframe, bevy_egui, and game-engine integrations use.

- `Context::run(RawInput, impl FnMut(&Context)) -> FullOutput`: [`egui::Context::run`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.run). `begin_pass`/`end_pass` are the manual alternative: [`Context::begin_pass`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.begin_pass).
- `Context::tessellate(Vec<ClippedShape>, pixels_per_point) -> Vec<ClippedPrimitive>`: [`Context::tessellate`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.tessellate).
- `FullOutput` carries `platform_output`, `textures_delta`, `shapes`, `pixels_per_point`, and `viewport_output`: [`egui::FullOutput`](https://docs.rs/egui/0.32.3/egui/struct.FullOutput.html), source [`egui/src/data/output.rs:5-36`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#5-36).
- `TexturesDelta` tells the backend what to upload before painting and free after: [`epaint::TexturesDelta`](https://docs.rs/epaint/0.32.3/epaint/textures/struct.TexturesDelta.html), source [`epaint/src/textures.rs:275-299`](https://docs.rs/epaint/0.32.3/src/epaint/textures.rs.html#275-299).
- A tessellated frame is `Vec<ClippedPrimitive>`, each with a clip rect and either a `Mesh` or a `PaintCallback`: [`epaint::ClippedPrimitive`](https://docs.rs/epaint/0.32.3/epaint/struct.ClippedPrimitive.html), [`epaint::Primitive`](https://docs.rs/epaint/0.32.3/epaint/enum.Primitive.html).
- `Mesh` is `indices: Vec<u32>`, `vertices: Vec<Vertex>`, `texture_id`: [`epaint::Mesh`](https://docs.rs/epaint/0.32.3/epaint/struct.Mesh.html), source [`epaint/src/mesh.rs:45-62`](https://docs.rs/epaint/0.32.3/src/epaint/mesh.rs.html#45-62).
- `PlatformOutput` carries clipboard/open-url commands, cursor icon, IME rects, output events, and (feature-gated) accesskit updates: [`egui::PlatformOutput`](https://docs.rs/egui/0.32.3/egui/struct.PlatformOutput.html), source [`egui/src/data/output.rs:100-165`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#100-165).
- `ViewportOutput` carries `repaint_delay` and viewport commands: source [`egui/src/viewport.rs:1162-1194`](https://docs.rs/egui/0.32.3/src/egui/viewport.rs.html#1162-1194).

eframe and `egui-winit` perform this loop for the host (gather `RawInput`, run, upload textures, paint), but they own OS windows/event loops and are not usable inside a guest module; the guest owns only the `Context` and the wire protocol. The integration contract is documented in the egui crate docs ([Integrating with egui](https://docs.rs/egui/0.32.3/egui/)).

**What this buys authors:** they can implement `egui::Widget` themselves, call `ui.add(my_widget)`, use `Grid`, `ScrollArea`, `TextEdit`, `Window`, `Area`, custom `Painter` shapes, animations, tooltips, and all normal egui layout. None of that needs host cooperation because it all executes inside the extension's own `Context`.

**Multi-pass caveat:** `Context::run` may call the UI closure more than once in a frame when `request_discard` is used (for example by `Grid` on its first frame); it runs at most `Options::max_passes` times ([`Context::run`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.run)). Extension draw code that mutates persistent state must be idempotent, or the extension should set `max_passes = 1`. The host must not assume one closure invocation per frame.

**What it does not buy:** the extension still cannot see or mutate host `ProjectDoc`, host widgets, host focus order, or host accessibility. Its UI is a clipped sub-surface. `Window`/`Area`/tooltip geometry that extends outside the panel must either be clipped by the host or handled by a separate overlay/z-order protocol; simple panel compositing means popups cannot float over host chrome. Interaction is routed only while the panel is focused/hovered.

### A.2 Wire format: what serializes and what must be hand-encoded

egui 0.32 has a `serde` feature that derives `Serialize`/`Deserialize` on many data types. The derives are visible in the source:

| Type | Serde? | Evidence |
|---|---|---|
| `RawInput`, `ViewportInfo`, events, modifiers | Yes | `RawInput` source derive: [`egui/src/data/input.rs:21-23`](https://docs.rs/egui/0.32.3/src/egui/data/input.rs.html#21-23) |
| `PlatformOutput`, `OutputCommand`, `OutputEvent`, `IMEOutput`, `CursorIcon` | Yes | derives at [`egui/src/data/output.rs:70-107`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#70-107) |
| `Mesh`, `Vertex` | Yes | derives at [`epaint/src/mesh.rs:10-48`](https://docs.rs/epaint/0.32.3/src/epaint/mesh.rs.html#10-48) |
| `TextureId` | Yes | derive at [`epaint/src/textures.rs:96-100`](https://docs.rs/epaint/0.32.3/src/epaint/textures.rs.html#96-100) |
| `TexturesDelta`, `ImageDelta`, `ColorImage`, `ImageData`, `TextureOptions` | Yes | derives at [`epaint/src/textures.rs:275`](https://docs.rs/epaint/0.32.3/src/epaint/textures.rs.html#275), [`epaint/src/image.rs:14-15`](https://docs.rs/epaint/0.32.3/src/epaint/image.rs.html#14-15), [`epaint/src/image.rs:406-410`](https://docs.rs/epaint/0.32.3/src/epaint/image.rs.html#406-410) |
| `Style`, `Visuals`, `FontDefinitions`, `FontData` | Yes | derives at [`egui/src/style.rs:239-242`](https://docs.rs/egui/0.32.3/src/egui/style.rs.html#239-242), [`epaint/src/text/fonts.rs:110-112`](https://docs.rs/epaint/0.32.3/src/epaint/text/fonts.rs.html#110-112), [`epaint/src/text/fonts.rs:248-251`](https://docs.rs/epaint/0.32.3/src/epaint/text/fonts.rs.html#248-251) |
| `FullOutput` | **No** | only `#[derive(Clone, Default)]`: [`egui/src/data/output.rs:5-9`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#5-9) |
| `ClippedPrimitive`, `Primitive`, `Shape`, `TextShape`, `Galley` | **No** | `ClippedPrimitive`/`Primitive` only `Clone, Debug`: [`epaint/src/lib.rs:132-147`](https://docs.rs/epaint/0.32.3/src/epaint/lib.rs.html#132-147); `Shape` includes `PaintCallback`: [`epaint/src/shapes/shape.rs:27-71`](https://docs.rs/epaint/0.32.3/src/epaint/shapes/shape.rs.html#27-71) |
| `ViewportOutput` | **No** | source struct has no derive: [`egui/src/viewport.rs:1162`](https://docs.rs/egui/0.32.3/src/egui/viewport.rs.html#1162) |
| `accesskit::TreeUpdate` | Feature-dependent; not enabled in this workspace | `PlatformOutput.accesskit_update` exists only with `accesskit`: [`egui/src/data/output.rs:144-148`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#144-148); the workspace's eframe features are only `default_fonts`, `glow`, `wgpu`, and egui uses its defaults; no `accesskit`: [`Cargo.toml:41-47`](../Cargo.toml#L41-L47) |

That yields a small hand-written envelope rather than trying to serialize `FullOutput`:

```text
EguiFrame {
  pixels_per_point: f32,
  primitives: Vec<{ clip_rect: Rect, mesh: Mesh }>, // callbacks skipped/rejected
  textures_delta: TexturesDelta,                    // serde
  platform_output: PlatformOutput,                  // serde, filtered by host
  repaint_after_secs: f64,                          // from ViewportOutput.repaint_delay
  used_rect: Rect,                                  // optional layout result
}
```

`RawInput` (serde) goes the other way. Use a compact binary codec (`postcard`, `bincode`, or a hand-written LE encoding) rather than JSON: a `Mesh` is numeric arrays, and JSON inflates them badly. `serde` support makes a binary codec straightforward; `bincode`/`postcard` are safe Rust and fit the no-unsafe rule. The existing JSON ABI is fine for control messages but not for per-frame meshes.

### A.3 Host compositing without unsafe

The host does not need a second renderer or unsafe access to `egui-wgpu`/`egui_glow` internals. There is a safe, backend-agnostic path:

1. Upload the extension's `ImageDelta`s into the host's texture manager with `Context::load_texture` / `TextureHandle::set` / `TextureHandle::set_partial`: [`Context::load_texture`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.load_texture), [`epaint::TextureHandle`](https://docs.rs/epaint/0.32.3/epaint/struct.TextureHandle.html). `TextureHandle` keeps the texture alive; the last clone frees it: [`epaint/src/texture_handle.rs:15-29`](https://docs.rs/epaint/0.32.3/src/epaint/texture_handle.rs.html#15-29).
2. Remap each extension `TextureId` to the host `TextureHandle::id()` for that extension panel. This is required because `TextureId::Managed(0)` is the font atlas in every egui context: [`epaint::TextureId`](https://docs.rs/epaint/0.32.3/epaint/enum.TextureId.html). egui itself documents that multiple viewports sharing one texture namespace is "a problem" and hacks around it by re-uploading the font atlas: [`egui/src/context.rs:2447-2458`](https://docs.rs/egui/0.32.3/src/egui/context.rs.html#2447-2458). Handle `TexturesDelta.free` by dropping the corresponding host handle; never let a guest-chosen id free a host texture.
3. Add each mesh to the host painter as `egui::Shape::Mesh(Arc<Mesh>)`, under a clip rect for the panel: [`egui::Painter::add`](https://docs.rs/egui/0.32.3/egui/struct.Painter.html#method.add), [`epaint::Shape::Mesh`](https://docs.rs/epaint/0.32.3/epaint/enum.Shape.html#variant.Mesh). The host's existing egui renderer then draws it; no wgpu/glow-specific code.
4. The host's normal frame already emits a `TexturesDelta` containing the uploaded extension textures; eframe/egui-wgpu/egui_glow upload them before painting. Renderer contracts: [`egui_wgpu::Renderer::update_texture`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html#method.update_texture), [`free_texture`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html#method.free_texture), [`update_buffers`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html#method.update_buffers), [`render`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html#method.render); [`egui_glow::Painter::paint_primitives`](https://docs.rs/egui_glow/0.32.3/egui_glow/painter/struct.Painter.html#method.paint_primitives), [`set_texture`](https://docs.rs/egui_glow/0.32.3/egui_glow/painter/struct.Painter.html#method.set_texture).

This is the key simplification: the host does not need to understand extension meshes at the renderer level; it turns them into ordinary egui shapes and lets its own backend do the work. A local headless test (throwaway temp crate, not committed) confirmed `ctx.load_texture` -> `TextureHandle::id()` -> `painter.add(Shape::Mesh)` -> `ctx.tessellate` preserves the remapped texture id and the foreign mesh. The same path uses no unsafe code.

An alternative is a per-extension `egui_wgpu::Renderer` or `egui_glow::Painter` with its own texture namespace. That avoids host texture handles but requires backend-specific render-pass integration and does not help the browser glow/wgpu split. It is more work for no functional gain.

### A.4 Input, focus, IME, clipboard

`RawInput` is serde-friendly. The host must build one per panel per frame:

- `screen_rect`: the panel rect (panel-local or absolute; choose one and document it).
- `time`: host monotonic time. `wasm32-unknown-unknown` std has no reliable clock ([`plan/BROWSER_BUILD.md:108-110`](BROWSER_BUILD.md#L108-L110), [Rust platform support](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)); supply it from the host.
- `modifiers`, pointer events, scroll, key/text/IME events, `focused`.
- `viewports[ROOT].native_pixels_per_point`: the host DPI scale.
- `max_texture_side`: the host renderer's limit.

The extension calls `Context::run`, which consumes those events. The host must then act on `PlatformOutput`:

- **Cursor**: apply `cursor_icon`.
- **Clipboard**: execute `OutputCommand::CopyText`/`CopyImage`; inject paste as `Event::Paste` on the next frame. The extension cannot touch the OS clipboard itself.
- **Open URL**: allow-list or reject; an extension must not open arbitrary URLs.
- **IME**: if `PlatformOutput.ime` is `Some`, translate its rects to screen coordinates and call the platform IME API (`egui-winit`/winit `set_ime_cursor_area`); forward `Event::Ime` back. Without this, CJK/IME text entry is broken. `IMEOutput` is documented as the text-edit rect and cursor rect: [`egui::IMEOutput`](https://docs.rs/egui/0.32.3/egui/output/struct.IMEOutput.html).
- **Keyboard ownership**: use `Context::wants_keyboard_input`/`wants_pointer_input` from the extension frame to suppress host shortcuts while the panel owns focus. These are normal `Context` queries: [`Context::wants_keyboard_input`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.wants_keyboard_input).
- **Focus**: only pass `focused: true` and key events when the host has given keyboard focus to the panel. Otherwise the extension's `TextEdit` may steal/duplicate input.

This is bespoke but bounded work. It is the main reason Option A is "medium-large", not "small".

### A.5 Font atlas, textures, and module size

The extension needs fonts to lay out and tessellate text. If it uses egui's default fonts, the module embeds them. A throwaway release build of egui 0.32.3 + `default_fonts` compiled to `wasm32-unknown-unknown` produced:

- **2.75 MB** `.wasm` (2,745,796 bytes), **zero imports**, exports `memory` and the test entry point.
- Initial linear memory 39 pages (2.4 MiB); after the first frame ~5.5 MiB.
- First frame 14 ms in a core-wasm VM; subsequent frames 0.1-0.2 ms for a panel with a heading, button, label, `TextEdit`, and a 20-row `ScrollArea`.
- 300-564 vertices and 696-1092 indices for that panel; the initial `TexturesDelta` contained a **1,048,576-byte** (512x512 RGBA) font atlas delta.

Consequences:

- The existing 256 KiB `MAX_OUTPUT_BYTES` cap ([`crates/notwig-ext-api/src/draw.rs:8`](../crates/notwig-ext-api/src/draw.rs#L8)) is too small for the initial font atlas. Either add a separate texture channel with its own budget, raise the cap for `ui.egui` extensions, or force a smaller `max_texture_side`/`reduce_texture_memory`. A 1 MiB initial texture delta is normal, not an attack.
- The 64 MiB store limit ([`crates/notwig-ext/src/abi.rs:12`](../crates/notwig-ext/src/abi.rs#L12)) is comfortable for one panel, but each panel context has its own font atlas and egui state. Many open extension panels multiply memory. Consider one UI store per extension (with multiple `Area`s) or a hard cap on concurrent egui panels.
- Module size is well under the 64 MiB entrypoint limit ([`docs/02_extensions/04_panel_api.md:199`](../docs/02_extensions/04_panel_api.md#L199)). Load time is dominated by compiling a ~2.75 MB module; do it off the UI thread, as the current registry already does for DSP modules.
- The host's fonts and the extension's fonts must match for visual consistency. Send `FontDefinitions` (serde) once, or both sides use the same egui default fonts. If the host later adds a custom font, the extension must receive it; otherwise text metrics and glyphs diverge.
- Use `TextureHandle::set_partial` for font-atlas deltas so the host does not reallocate a 512x512 texture every frame. `TextureHandle` supports full and partial updates: [`TextureHandle::set`](https://docs.rs/epaint/0.32.3/epaint/struct.TextureHandle.html#method.set), [`set_partial`](https://docs.rs/epaint/0.32.3/epaint/struct.TextureHandle.html#method.set_partial).

### A.6 DPI, clipping, theming, repaint

- **DPI**: set `ViewportInfo::native_pixels_per_point` from the host; read `FullOutput::pixels_per_point`; tessellate with that exact value. `Context::pixels_per_point` is `zoom_factor * native_pixels_per_point`: [`Context::pixels_per_point`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.pixels_per_point), [`Context::set_pixels_per_point`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.set_pixels_per_point). Feathering/AA depends on it, so a mismatch produces visibly different edges. The host renderer still uses its own `pixels_per_point`; mesh coordinates are in points.
- **Clipping**: give the extension a `screen_rect` for the panel. `FullOutput`/`ClippedPrimitive` clip rects are in that coordinate space. The host intersects every primitive's clip rect with the panel rect and adds the mesh under `Painter::with_clip_rect(panel_rect)`. If using panel-local coordinates, translate mesh vertex positions by `panel_rect.min` (or use absolute screen coordinates and skip translation). The host must clip, not trust the extension's clip rect.
- **Theming**: `Style`/`Visuals`/`FontDefinitions` are serde. Send the host's current style once and on theme change; the extension calls `Context::set_style`/`set_fonts`. `Style::number_formatter` is `serde(skip)` and falls back to the default formatter ([`egui/src/style.rs:295-296`](https://docs.rs/egui/0.32.3/src/egui/style.rs.html#295-296)), so custom number formatting will not transfer. Without this, extension panels look like stock egui, not Notwig.
- **Repaint**: an extension calling `Context::request_repaint` only affects its own context. The host must read `ViewportOutput::repaint_delay`, serialize it (e.g. seconds/nanos), and call the host `Context::request_repaint_after`. Otherwise meters/animations freeze. `ViewportOutput.repaint_delay` is documented at [`egui/src/viewport.rs:1187-1193`](https://docs.rs/egui/0.32.3/src/egui/viewport.rs.html#1187-1193); host request API: [`Context::request_repaint_after`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.request_repaint_after).

### A.7 Accessibility

This is the biggest product gap. The workspace does not enable egui's `accesskit` feature (`eframe` uses only `default_fonts`, `glow`, `wgpu`, and egui uses defaults): [`Cargo.toml:41-47`](../Cargo.toml#L41-L47). Without it, `PlatformOutput` has no `accesskit_update` ([`egui/src/data/output.rs:144-148`](https://docs.rs/egui/0.32.3/src/egui/data/output.rs.html#144-148)), so an extension-rendered panel is opaque to screen readers. Current typed host widgets can be made accessible because the host creates them (once accesskit is enabled); an extension-rendered panel cannot be without a tree protocol.

If accesskit is enabled on both sides, the extension's `TreeUpdate` must be forwarded, re-rooted with unique node ids per panel, and its actions routed back. That is possible but is a separate protocol, not a serde field you can blindly forward. Recommendation: do not claim accessibility parity for `ui.egui` panels until this is designed; keep primary controls as host widgets where accessibility matters.

### A.8 Security and crash isolation

Wasmtime's sandbox is real: linear-memory isolation, checked memory accesses, no raw syscalls, imports are the only capability, and no undefined behavior in wasm execution. See [Wasmtime security](https://docs.wasmtime.dev/security.html). A trap (panic/abort, fuel exhaustion, out-of-bounds) is reported as an error at the host call boundary, not across the host process; `wasm32-unknown-unknown` defaults to `panic=abort` ([platform support](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)). The current host already resets fuel per call and handles traps for DSP ([`crates/notwig-ext/src/abi.rs:153-155`](../crates/notwig-ext/src/abi.rs#L153-L155), [`crates/notwig-ext/src/device.rs:206-241`](../crates/notwig-ext/src/device.rs#L206-L241)).

But raw meshes introduce a **new validation surface** that the current draw IR does not have:

- Validate every vertex position/UV/color is finite.
- Validate every index is `< vertices.len()`, indices length is a multiple of 3, and vertex/index counts are capped.
- Reject `Primitive::Callback`; it contains a `PaintCallback` with host-side callback data and cannot cross a sandbox ([`epaint::Primitive`](https://docs.rs/epaint/0.32.3/epaint/enum.Primitive.html)). Only `Primitive::Mesh` is wire-safe.
- Cap texture dimensions and reject `TextureId::User` unless explicitly mapped to a host-provided texture. Never let an extension name an arbitrary host texture id.
- Cap clip rects, total output bytes, texture deltas, mesh count, and vertices per frame. A malicious extension can otherwise exhaust GPU memory or trigger a device loss with out-of-range indices; egui's renderer consumes the index/vertex arrays directly and does not validate them for you ([`egui_wgpu::Renderer::update_buffers`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html#method.update_buffers)).
- Keep fuel/epoch budgets. Tessellation is CPU work in the guest; a pathological UI can burn fuel or stall the UI thread if the call is synchronous. The current 20M fuel per call may need to be raised for draw calls, or a separate draw budget used. Prefer a wall-clock epoch deadline as well as fuel; a CPU-instruction budget does not bound font rasterization time. The same tradeoff already exists for DSP ([`plan/EXTENSIONS.md:62-64`](EXTENSIONS.md#L62-L64)).

Native dylib variant: the same wire protocol can work, but the extension runs in-process, so none of the above isolation applies. A bad mesh can still crash the host renderer, and a bad dylib can corrupt the process. Only load trusted native extensions, as the docs already say ([`docs/02_extensions/04_panel_api.md:211-216`](../docs/02_extensions/04_panel_api.md#L211-L216)).

### A.9 Performance and size, measured

The local experiment above is deliberately small, but it establishes the order of magnitude:

- **Module**: 2.75 MB (egui + epaint + default fonts), zero imports.
- **Memory**: 2.4 MiB initial, ~5.5 MiB after one frame.
- **CPU**: ~14 ms first frame (font atlas/glyph rasterization), 0.1-0.2 ms subsequent frames for a simple 24-widget panel in a release wasm build.
- **Wire**: ~300-564 vertices, ~700-1100 indices, ~6-22 KiB of vertex/index data per frame if binary-encoded; 1 MiB initial font atlas.
- **Risk**: first-frame latency can drop a frame when a panel opens. Prewarm off the UI thread is hard because egui is not `Send`-friendly across a wasm store, but the host can warm the module/store and run a dummy frame on a worker before showing the panel, or accept a one-frame hitch.

`wasmtime`/Cranelift is a different JIT; do not assume the V8 numbers transfer exactly. Treat them as an order-of-magnitude estimate and benchmark on wasmtime. The current DSP path already relies on Cranelift ([`crates/notwig-ext/src/abi.rs:33-38`](../crates/notwig-ext/src/abi.rs#L33-L38)). The font atlas transfer and first-frame rasterization are the real costs.

### A.10 Platform support

- **Native macOS**: `wasmtime` runs core wasm; the module has zero imports and only needs `memory`, so instantiation is trivial. The current extension host already does this for DSP ([`crates/notwig-ext/src/abi.rs:60-151`](../crates/notwig-ext/src/abi.rs#L60-L151)).
- **Browser**: `wasmtime` cannot run inside `wasm32-unknown-unknown` ([`plan/BROWSER_BUILD.md:213-215`](BROWSER_BUILD.md#L213-L215)). The browser path already instantiates core wasm via `js_sys::WebAssembly` ([`crates/notwig-ext/src/web/host.rs:77`](../crates/notwig-ext/src/web/host.rs#L77), [`crates/notwig-ext/src/web/instance.rs:80`](../crates/notwig-ext/src/web/instance.rs#L80)). The same guest egui module can run there because it has no imports, but the host-side bridge must be implemented in the browser host (Rust/wasm calling JS `WebAssembly`, or JS glue). Large byte arrays should cross via memory views, not JSON.
- **Component model**: not required. The guest is a plain core module with no imports, which is the simplest browser-compatible shape. WIT/component model would add tooling and is a different binary format ([Why the Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html)).

### A.11 Effort and risk

- **MVP for device panels**: new wire types + guest helper + host `EguiPanelHost` + texture remap + input bridge + mesh validation + tests. Roughly 2-4 engineer-weeks for one platform, more for native/browser parity.
- **Production-grade**: add IME, clipboard, focus arbitration, style sync, repaint, a11y decision, fuzzing of the mesh decoder, and performance limits. Another few weeks.
- **Risk**: medium. The egui integration itself is well-trodden; the bespoke parts are the host input/focus bridge, texture namespace, and untrusted mesh validation.
- **Payoff**: extension authors get the actual egui API and ecosystem knowledge, with no new UI DSL to learn.

## Option B: pass the host `egui::Context`/`Ui` into a dylib

### B.1 Why direct pointer passing fails

`egui::Context` is `Arc<RwLock<ContextImpl>>`, and `ContextImpl` is private. It is cheap to clone and all methods use `&self`, but the internals are not an FFI type: [`egui::Context`](https://docs.rs/egui/0.32.3/egui/struct.Context.html) (source [`egui/src/context.rs:754-755`](https://docs.rs/egui/0.32.3/src/egui/context.rs.html#754-755)). `egui::Ui` is a frame-scoped object tied to a context and a pass; it carries layout/interaction state and is not an FFI type.

Rust's layout and ABI guarantees are the blocker:

- Type layout can change with every compilation; `repr(Rust)` only guarantees soundness properties (field alignment/order constraints), not offsets: [Rust Reference: Type layout](https://doc.rust-lang.org/reference/type-layout.html).
- The Rust ABI "offers no stability guarantees": [Rust Reference: external blocks, ABI](https://doc.rust-lang.org/reference/items/external-blocks.html#the-abi).
- `extern "C"` does not make a Rust struct layout stable. A `#[repr(C)]` wrapper can stabilize fields, but `Context`/`Ui`/`Response`/`Widget` are not `repr(C)` and cannot be made so by an external crate.
- `abi_stable` only helps types that implement `StableAbi`; it exists for Rust-to-Rust FFI with load-time layout checks, and it explicitly targets plugin systems "without support for unloading": [abi_stable](https://docs.rs/abi_stable/0.11.3/abi_stable/). It cannot retroactively make egui's types stable.
- `stabby` likewise stabilizes types you annotate; it cannot annotate egui's types, and it documents its own caveats (trait-object vtable leaks, nightly tradeoffs): [stabby](https://docs.rs/stabby/36.2.2/stabby/).

Even if you transmute a pointer, both sides must agree on the exact egui version, feature flags (`accesskit` changes layout), compiler version, optimization level, and allocator. egui explicitly says it is in heavy development with breaking changes each version: [egui crate docs](https://docs.rs/egui/0.32.3/egui/). A plugin ecosystem cannot pin all of that.

### B.2 Unsafe and `forbid(unsafe_code)`

The host UI/app crates forbid unsafe code ([`crates/notwig-ui/src/lib.rs:1`](../crates/notwig-ui/src/lib.rs#L1), [`crates/notwig-app/src/lib.rs:1`](../crates/notwig-app/src/lib.rs#L1)); the workspace denies it globally ([`Cargo.toml:108`](../Cargo.toml#L108)). The only existing exception is the native dylib adapter ([`crates/notwig-ext/src/native.rs:1`](../crates/notwig-ext/src/native.rs#L1)), which uses `libloading` and raw function pointers. Passing a host `Context` pointer would either:

- push unsafe into `notwig-ui`/`notwig-app` (violates the architecture), or
- create a new unsafe bridge crate that dereferences a pointer whose layout is not guaranteed (still unsound across version skew), or
- require the extension to link the exact same egui and use `unsafe` on its side too.

`libloading` itself is unsafe by nature; the project already isolates that to one module. Expanding it to egui is a different risk class because egui types are not designed for FFI.

### B.3 Isolation, crash behavior, platform

- Native dylibs run in the host process with full address space access. Unwinding across a non-`unwind` `extern "C"` boundary is undefined behavior unless caught ([Rust Reference: unwinding](https://doc.rust-lang.org/reference/items/functions.html#unwinding)); a segfault kills Notwig. There is no sandbox. This is acceptable only for first-party or trusted extensions, as the docs already state ([`docs/02_extensions/04_panel_api.md:211-216`](../docs/02_extensions/04_panel_api.md#L211-L216)).
- Browser is impossible: no dylibs.
- macOS code signing/notarization and hardened-runtime library validation add operational friction for third-party dylibs ([Apple: disable-library-validation entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation)).

### B.4 Prior art shows the cost

`egui_wings` is the closest real project: it lets WASM plugins use normal `egui::Window`/`Ui` calls against a context synchronized with the host. But it does not pass a pointer across the wasm boundary. It snapshots the host context into the guest, runs egui there, and snapshots back. Its source says it "abuses compiler behavior to get access to egui's private state" and is a "temporary hack until [egui PR 4930]": [`egui_wings/src/lib.rs:63-65`](https://docs.rs/egui_wings/0.7.0/src/egui_wings/lib.rs.html#63-65). Its private mirror uses `unsafe { transmute(context) }` and size assertions: [`egui_wings private_hack`](https://docs.rs/crate/egui_wings/0.7.0/source/src/private_hack/mod.rs). It is pinned to egui 0.33 and depends on private internals. That is exactly the kind of coupling a `forbid(unsafe_code)` project should avoid, but it proves demand and shows why naive context sharing is fragile.

### B.5 Host-proxied egui calls (a variant of B)

Instead of passing a pointer, the host can expose C ABI functions that operate on a host-owned `Context`/`Ui` handle table, and the extension calls them:

```text
host_ui_begin_frame(panel, raw_input_ptr, len)
host_ui_horizontal_begin(panel)
host_ui_label(panel, text_ptr, len)
host_ui_button(panel, text_ptr, len) -> response_id
host_ui_text_edit(panel, id, text_ptr, len) -> ...
host_ui_horizontal_end(panel)
host_ui_end_frame(panel) -> frame_output
```

This is technically possible and keeps the host's real egui state. It is also a remote-control reimplementation of egui:

- Every method and type needs a proxy. `ui.add(impl Widget)` cannot cross; only host-provided widget constructors can. Custom `Widget` implementations cannot run on the host because they are Rust code in the guest.
- Closures (`ui.horizontal(|ui| ...)`) become begin/end calls or host callbacks into the guest. Host callbacks re-enter wasm, complicating borrows, fuel, and re-entrancy. Guest begin/end calls require the host to maintain a `Ui` stack across FFI calls.
- `Response` is a large struct with methods and `Arc`/`Id` internals; returning a copy is possible but loses `ui`-bound behavior. `Sense`, `Id`, `Rect`, `Color32` can be flattened; `WidgetText`, `Galley`, `TextBuffer`, `Style` and `Memory` cannot be exposed wholesale.
- Performance is one host call (plus canonical-ABI copies) per widget/layout step. That is fine for tens of widgets, but it is strictly worse than Option C, which designs the same idea with a smaller, typed surface.
- It still does not give arbitrary host UI: the extension is calling a facade, not receiving `Ui`.

Verdict: not worth building separately. If host-owned UI is the goal, Option C is the disciplined version of this idea.

## Option C: expand typed host imports (WIT/component model or core-wasm ABI)

### C.1 What is possible

The host keeps the real egui `Context`/`Ui` and exposes a typed API to the extension. In WIT, the interface can use records, variants, lists, strings, resources, and (newer) futures/streams; `wasmtime::component::bindgen!` generates Rust host traits from a WIT world, while [`wit-bindgen`](https://github.com/bytecodealliance/wit-bindgen) generates the guest bindings: [`wasmtime::component`](https://docs.rs/wasmtime/34.0.2/wasmtime/component/), [`bindgen!`](https://docs.rs/wasmtime/34.0.2/wasmtime/component/macro.bindgen.html). The component model's value proposition is exactly this: richer types than i32/f32, a canonical ABI, and imports/exports as the only interaction: [Why the Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html), [WIT](https://component-model.bytecodealliance.org/design/wit.html), [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html).

A useful first surface:

```text
allocate(size) -> rect
horizontal(add: func) / vertical(add: func)
label(text) / section(text)
button(text) -> response
knob(param-id, value, default, bipolar, label) -> response
toggle(param-id, active, label) -> response
text-edit(id, text, single-line) -> edit-result
scroll-area(id, max-height, add: func)
window(id, title, open, add: func)
push-clip(rect) / pop-clip()
measure-text(text, font) -> size
style() -> style-snapshot
set-param-normalized(param-id, value)
```

The host runs all of these through its normal `Ui`, so theming, hit-testing, focus, accessibility, and parameter validation stay host-owned. This is a direct extension of the current `PanelCommand` model, not a new architecture.

**Responsibilities.** The host implements each WIT function against a per-panel `Ui` handle/stack, owns focus and text-measurement caches, snapshots styles, and validates every parameter action. The extension calls the generated bindings, keeps its own view state, and must not hold host pointers across calls. Begin/end layout calls must be balanced; the host should trap/recover if they are not.

### C.2 What is not possible

- `ui.add(my_widget)` for an arbitrary guest `impl Widget`: the widget code is guest Rust; it cannot run inside the host's `Ui` without shipping a wasm function pointer and calling back per widget, which is a different (and much harder) design.
- Arbitrary `Ui`/`Context` methods: every method you expose is a new WIT function and a host implementation. Full egui has hundreds of methods and types, many generic (`Grid`, `Table`, `Sides`, `StripBuilder`, `UiBuilder`, `AtomLayout`), many with closures, many producing `Response` methods that read host state.
- `Painter`/`Shape` graph: you can expose primitive calls (`rect_filled`, `line`, `text`, `mesh`?) but not the full shape/tessellation API. Exposing raw meshes would reintroduce Option A's validation surface.
- Text layout internals: `Galley`, `Fonts`, `TextBuffer`, custom text layout, text selection internals. A `measure-text` host call covers most layout needs but not custom shaping.
- `Style`/`Memory` wholesale: expose curated style snapshots, not the full structs with function pointers.
- Full accessibility: host widgets are accessible, but a guest-composed custom widget only gets accessibility if the host API exposes semantic roles/labels for it.

### C.3 Effort, performance, and browser

- **Effort**: incremental. A layout + text + common-widget subset (label, button, knob, toggle, text edit, scroll) is 1-2 engineer-weeks. Broad coverage is months, and there is no "done" state matching full egui. Every addition is a WIT type + host impl + guest wrapper + test.
- **Performance**: one host call per widget/layout step. For a panel with tens of widgets this is negligible. Canonical-ABI string/list copies are the main overhead; cache text measurement and style snapshots. Avoid per-pixel host calls.
- **Browser**: the current browser host uses core wasm with JS import objects ([`crates/notwig-ext/src/web/instance.rs:80`](../crates/notwig-ext/src/web/instance.rs#L80)). Core-wasm imports work there. WIT/component model is a different binary format and needs component tooling/transpilation; it is a reasonable native-only choice but adds a second path for the browser. A pragmatic approach is a typed core-wasm ABI (similar to today's exports) for both, with WIT as an optional native authoring layer.
- **Security**: best of the options. The host executes only known operations, validates arguments, and never accepts raw meshes or texture ids. No unsafe.

### C.4 Verdict

Option C is the right long-term path for host-integrated, themed, accessible UI. It cannot satisfy "full egui API" literally, but it can cover the 80-95% of panel controls that should be host-owned. Pair it with Option A for the remaining custom-visualization cases.

## Prior art

| Project | What it does | Lesson for Notwig |
|---|---|---|
| [`egui_wings`](https://docs.rs/egui_wings/0.7.0/egui_wings/) | Shares an `egui::Context` between host and multiple guest WASM modules; guest calls normal egui `Window`/`Ui`; snapshots context state across the boundary. | Proves demand and that egui can run in a guest; also proves context sharing needs private-state access (`unsafe transmute`) and exact version coupling. Not compatible with `forbid(unsafe_code)`. |
| [`zed-industries/embedded_gpui`](https://github.com/zed-industries/embedded_gpui) | Runs GPUI itself inside a `wasm32-wasip2` component; wasmtime host on a background worker; WIT protocol; host replays display lists as native GPUI primitives; text shaping on the host; never calls wasm from the frame path; epoch/memory caps. | Closest architectural precedent for Option A: guest UI framework + host display-list replay is viable at scale. It also shows the alternative of host-side text shaping, which avoids font-atlas transfer but requires host text-measure callbacks. Experimental, not a supported API. |
| [`bevy_egui`](https://docs.rs/bevy_egui/latest/bevy_egui/) | Multiple egui contexts attached to cameras/windows, including split-screen. | Compositing multiple egui contexts in one app is normal. But each context has its own font texture; the host must manage texture namespaces. |
| [`hot-lib-reloader` egui example](https://docs.rs/hot-lib-reloader/latest/hot_lib_reloader/) | Hot-reloads a native egui/eframe app from a dylib. | Native dylib egui works for development, but it assumes the same compiler/version and is not a plugin sandbox. |
| [`wasm-audio-examples`](https://github.com/wasm-audio/wasm-audio-examples) | A host egui application loads component-model WASM audio plugins. | Component-model WASM plugins are viable for DSP/control; the example is not guest UI, and guest egui UI is a separate problem. |
| [`ovlo/oxide`](https://github.com/ovlo/oxide) | WASM guest emits draw/widget commands; an egui host replays them. | Same architecture as Notwig's current draw IR; safe but not full egui. |

## Recommendation

1. **Do not pass the host `Context`/`Ui`.** It is unsound across ABI/version boundaries, requires unsafe in forbidden crates, and gives no isolation. Native full-egui plugins should not run in the main process.
2. **Keep the typed draw IR/host widgets as the default.** It is the only path that is currently themed, validated, and browser-parity, and the only path that can be made accessible without a new tree protocol. Expand it (Option C) for controls that must be host-owned.
3. **Add Option A as an opt-in `ui.egui` capability**, wasm-only, for panels that genuinely need custom egui widgets/layout:
   - New ABI version or a separate export (e.g. `notwig_device_draw_egui`) so existing `PanelDraw` extensions are untouched.
   - Private `egui::Context` per panel, retained inside the extension's UI store; never share it with DSP stores.
   - Binary per-frame envelope; `Mesh`/`TexturesDelta`/`PlatformOutput`; reject callbacks.
   - Per-extension texture namespace via `Context::load_texture`/`TextureHandle`; remap ids; separate texture budget for the initial font atlas.
   - Input/focus/IME/clipboard bridge on the host; `wants_keyboard_input` to arbitrate shortcuts; repaint delay forwarding.
   - Strict mesh/texture validation and caps; treat the extension as untrusted even though egui generated the data.
   - Document the a11y limitation; keep critical controls as host widgets.
4. **If the goal is arbitrary host UI, Option C is the only safe answer.** Option A gives arbitrary *panel* UI, not arbitrary host UI. Overlaying host chrome, integrating with host focus order, and participating in the host accessibility tree require a host-owned API and a much larger protocol.
5. **Native dylibs:** the same guest-egui wire protocol can be supported for trusted native extensions, but it remains in-process and unsandboxed. Do not present it as equivalent to wasm isolation. The existing C ABI/draw-IR native path is the safer default.
6. **Testing:** WAT fixtures for the new ABI; a headless host test that uploads a texture, composes a foreign mesh, and asserts remapped ids; fuzz the mesh/texture decoder; cap tests; a first-frame/per-frame benchmark; a browser smoke test; an a11y review before claiming support.

## Appendix: local feasibility experiment (not committed)

A throwaway crate outside the repo compiled egui 0.32.3 with default fonts for `wasm32-unknown-unknown`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
egui = "=0.32.3"
```

The guest entry point creates a `Context`, runs `CentralPanel` with a heading, button, label, `TextEdit`, and a 20-row `ScrollArea`, calls `ctx.tessellate`, and returns the vertex count. Results:

```text
wasm size: 2,745,796 bytes
imports: []
exports: memory, run_frame
initial memory: 39 pages (2.4 MiB)
after first frame: 5.5 MiB
320x200:  13.93 ms, verts=300, idx=696, prims=2, texBytes=1,048,576
320x200:   0.22 ms, verts=300, idx=696, prims=2, texBytes=0
800x600:   0.13 ms, verts=564, idx=1092, prims=2, texBytes=0
2000x1000: 0.16 ms, verts=564, idx=1092, prims=2, texBytes=0
```

A second test used a headless host `egui::Context` to `load_texture`, take `TextureHandle::id()`, add a foreign `Mesh` as `Shape::Mesh`, tessellate, and assert the mesh/texture id survived. It passed under normal safe Rust:

```text
host_composites_foreign_mesh_with_remapped_texture ... ok
```

These are not wasmtime benchmarks (Node/V8 was used for the guest), but the guest has zero imports and only a `memory` export, so a core-wasm runtime such as wasmtime can instantiate it with no host functions. The numbers establish the order of magnitude: a few MB of module, a few MB of memory, sub-millisecond steady-state frames, and a ~1 MiB first-frame font atlas.

## Source index

Primary egui/epaint:

- [`egui::Context::run`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.run)
- [`egui::Context::tessellate`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.tessellate)
- [`egui::FullOutput`](https://docs.rs/egui/0.32.3/egui/struct.FullOutput.html)
- [`egui::RawInput`](https://docs.rs/egui/0.32.3/egui/struct.RawInput.html)
- [`egui::PlatformOutput`](https://docs.rs/egui/0.32.3/egui/struct.PlatformOutput.html)
- [`egui::IMEOutput`](https://docs.rs/egui/0.32.3/egui/output/struct.IMEOutput.html)
- [`epaint::TexturesDelta`](https://docs.rs/epaint/0.32.3/epaint/textures/struct.TexturesDelta.html)
- [`epaint::Mesh`](https://docs.rs/epaint/0.32.3/epaint/struct.Mesh.html)
- [`epaint::ClippedPrimitive`](https://docs.rs/epaint/0.32.3/epaint/struct.ClippedPrimitive.html)
- [`epaint::Primitive`](https://docs.rs/epaint/0.32.3/epaint/enum.Primitive.html)
- [`epaint::Shape`](https://docs.rs/epaint/0.32.3/epaint/enum.Shape.html)
- [`epaint::TextureId`](https://docs.rs/epaint/0.32.3/epaint/enum.TextureId.html)
- [`epaint::TextureHandle`](https://docs.rs/epaint/0.32.3/epaint/struct.TextureHandle.html)
- [`egui::Context::load_texture`](https://docs.rs/egui/0.32.3/egui/struct.Context.html#method.load_texture)
- [`egui::Painter::add`](https://docs.rs/egui/0.32.3/egui/struct.Painter.html#method.add)
- [`egui_wgpu::Renderer`](https://docs.rs/egui-wgpu/0.32.3/egui_wgpu/struct.Renderer.html)
- [`egui_glow::Painter`](https://docs.rs/egui_glow/0.32.3/egui_glow/painter/struct.Painter.html)
- [egui integration docs](https://docs.rs/egui/0.32.3/egui/)

ABI/FFI:

- [Rust Reference: type layout](https://doc.rust-lang.org/reference/type-layout.html)
- [Rust Reference: external blocks/ABI](https://doc.rust-lang.org/reference/items/external-blocks.html#the-abi)
- [abi_stable](https://docs.rs/abi_stable/0.11.3/abi_stable/)
- [stabby](https://docs.rs/stabby/36.2.2/stabby/)

WASM/component model:

- [Wasmtime security](https://docs.wasmtime.dev/security.html)
- [wasmtime::component](https://docs.rs/wasmtime/34.0.2/wasmtime/component/)
- [wasmtime::component::bindgen!](https://docs.rs/wasmtime/34.0.2/wasmtime/component/macro.bindgen.html)
- [Why the Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html)
- [WIT](https://component-model.bytecodealliance.org/design/wit.html)
- [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html)
- [wit-bindgen](https://github.com/bytecodealliance/wit-bindgen)
- [wasm32-unknown-unknown](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)

Prior art:

- [egui_wings](https://docs.rs/egui_wings/0.7.0/egui_wings/)
- [egui_wings private hack](https://docs.rs/crate/egui_wings/0.7.0/source/src/private_hack/mod.rs)
- [zed embedded_gpui](https://github.com/zed-industries/embedded_gpui)
- [bevy_egui](https://docs.rs/bevy_egui/latest/bevy_egui/)
- [hot-lib-reloader](https://docs.rs/hot-lib-reloader/latest/hot_lib_reloader/)
- [wasm-audio-examples](https://github.com/wasm-audio/wasm-audio-examples)
- [ovlo/oxide](https://github.com/ovlo/oxide)
