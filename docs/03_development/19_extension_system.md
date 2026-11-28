# Extension system

Status: the package/runtime ABI, lifecycle phases, contribution manifests,
built-in point registry, bottom panels, clipboard provider, and the in-process
typed interface layer are implemented. The package-side typed RPC export and
host scheduler are not wired yet, so typed package-to-package calls are still a
follow-up. This page is the single development reference for the extension
system; author-facing recipes live in [`docs/02_extensions`](../02_extensions/00_overview.md).

## 1. Model and invariants

An **interface** is a stable callable contract such as `dyn Panel`. An
**extension point** is a named registration scope that accepts one interface.
A **package extension** owns an id such as `com.example.midi-tools`. An
**implementation** is one registered object or factory on a point.

| Concept | Example | Identity |
| --- | --- | --- |
| Interface | `dyn Panel` | `com.notwig.api.panel@1` |
| Extension point | `TRACK_PANEL` | `com.notwig.track.panel` |
| Extension owner | Notwig Core | `notwig.core` |
| Package extension | MIDI tools | `com.example.midi-tools` |
| Implementation | `MidiMenu` | stable implementation id |

Rules:

- Interfaces and points are independent. One interface may be used by many
  points; one point may accept many implementations.
- All ABI identifiers are stable namespaced strings. Do not derive them from
  Rust `TypeId`, display names, paths, or discovery order.
- A package extension only registers and removes its own contributions. It
  cannot mutate another extension's rows.
- Canonical state stays in `ProjectDoc`. Extensions observe bounded snapshots
  and return validated edits/actions; they never mutate the document directly.
- The audio callback never enters generic extension RPC. Device DSP uses the
  dedicated block processing ABI instead.
- Built-ins use the same points as packages, but are owned by the fixed,
  non-uninstallable `notwig.core` / `Notwig Core` extension.

Point constants use the `EP_` prefix, where `EP` means extension point. The
current built-in points are:

| Constant | Point id | Cardinality | Purpose |
| --- | --- | --- | --- |
| `EP_NOTWIG_DEVICES` | `notwig.devices` | Many | Device specs and DSP |
| `EP_COMMANDS` | `notwig.commands` | Many | Host-owned command edits |
| `EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR` | `notwig.audio_editor.sub_editor` | Many | Audio-editor sub-editors |
| `EP_NOTWIG_BOTTOM_PANEL` | `notwig.bottom_panel` | Many, one open | Editor bottom panels |
| `EP_NOTWIG_CLIPBOARD` | `notwig.clipboard` | Single | Replaceable clipboard provider |

`ExtensionPoint<T>` is only a typed key. It stores a stable id and a
`Cardinality` (`Many` or `Single`) and contains no runtime, handle, or heap
state. `CoreExtensionRegistry` stores host-side implementations by point id and
returns `Arc<T>` today. `get_extensions` returns all active values;
`get_extension` returns exactly one and panics on zero or multiple;
`try_get_extension` is the recoverable form.

## 2. Crate boundaries

| Crate | Owns |
| --- | --- |
| `notwig-ext-api` | Guest SDK, wire types, lifecycle builders, point keys, typed RPC traits, panel frame validation |
| `notwig-ext-macros` | `#[notwig_ext(...)]`, `#[notwig_ext_trait]`, and `#[derive(ExtensionCall)]` expansion |
| `notwig-ext` | Package discovery, native/browser/WASM loading, host calls, transport instances, contribution descriptors |
| `notwig-ext-devices` | Bundled device extension pack |
| `cargo-notwig-ext` | Scaffolding and target-specific packaging |
| `notwig-ui` | Panel compositing, shared controls, core bottom panels, clipboard call sites |
| `notwig-app` | Host wiring, extension manager UI, workspace clipboard provider, command dispatch |
| `notwig-core` | Shared domain types such as `Clipboard` and `DeviceSpec` |

`notwig-ext-api` must stay free of host I/O, `wasmtime`, `libloading`, and
application state. `notwig-ext` owns loading and execution but not project
policy. The app composes hosts, registries, and UI.

## 3. Package lifecycle and registration

An installed package is a directory containing `extension.json`, ordered
entrypoints, and optional assets. Discovery parses manifests without running
extension code. Entrypoints are tried in order until a target loads and
describes every declared device. Unsafe paths, duplicate ids, unsupported ABI
versions, and oversized entrypoints are rejected.

The host runs phases in this order:

```text
discover manifests
  -> validate manifest and target list
  -> preload every runnable extension
  -> load every runnable extension
  -> read the runtime registration manifest
  -> resolve contributions against the manifest fallback
  -> build device specs and host contribution maps
  -> on unload/drop, call unload and destroy instances
```

Author entrypoints declare only the phase they need:

```rust
#[notwig_ext(preload)]
pub fn preload(extension: &mut PreloadBuilder) {
    extension.register_extension_point(TRACK_PANEL);
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<TrackDevice>(EP_NOTWIG_DEVICES);
    extension.register_extension_instance(MY_SINGLE_POINT, MyService::new());
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {
    // Release extension-owned resources. The host owns instance teardown.
}
```

`load` is required and generates the contribution ABI. `preload` is required
for a package that declares points. `unload` is optional. A crate root is wiring
only: module declarations, re-exports, imports, and the lifecycle functions,
with `load`, `unload`, and any reload hook last.

The registration manifest returned after load is JSON and contains declared
points plus device, command, audio-sub-editor, and bottom-panel descriptors.
Runtime descriptors take precedence over manifest `contributes` entries, which
remain a compatibility fallback. Device specs are merged into the normal device
registry and are stock-like to the rest of the app.

`register_extension_instance(point, instance)` keeps the actual typed instance
inside the guest runtime as a `Box<dyn ExtensionCall>`. `register_extension::<T>
(point)` constructs `T::default()` and registers it on the point interface
declared by `#[extension_call(Trait)]` on `T`. This is the single registration
path for devices, commands, audio sub-editors, bottom panels, and typed
interfaces. Device DSP still uses the dedicated block-processing ABI rather than
generic RPC. These registrations are listed in the runtime manifest; a typed
instance is currently not, because the package RPC export is not implemented
yet.

## 4. Typed interfaces and the call path

A typed interface is declared with an attribute macro:

```rust
#[notwig_ext_trait(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(
        &self,
        context: &ExtensionContext<'_>,
        input: PanelInput,
    ) -> Result<PanelFrame, ExtensionError>;
}
```

The macro generates:

- `PanelHandle`, a concrete proxy implementing `Panel`;
- `PanelRequest`, `PanelResponse`, and method ids;
- `PanelDispatcher`, which decodes a request, calls the implementation, and
  encodes a response;
- `ExtensionHandle for dyn Panel` with `type Handle = PanelHandle`;
- interface id/version metadata and a `Debug` handle implementation.

Implementations use the normal trait impl plus one derive on the concrete type:

```rust
#[derive(ExtensionCall)]
#[extension_call(Panel)]
struct MyPanel;

impl Panel for MyPanel {
    fn draw(
        &self,
        context: &ExtensionContext<'_>,
        input: PanelInput,
    ) -> Result<PanelFrame, ExtensionError> {
        // Build a frame; resolve other points through `context`.
        let _ = context;
        let _ = input;
        Ok(PanelFrame::default())
    }
}
```

Trait methods must take `&self` and a `&ExtensionContext<'_>` second argument.
They return `Result<T, ExtensionError>` with owned bitcode-serializable values;
generic methods, async, unsafe/extern methods, associated types, associated
consts, and borrowed wire values are rejected. The generated handle and
dispatcher use **bitcode** for RPC payloads. Other extension wire data, such as
panel frames and commands, still uses postcard; metadata is JSON.

Contribution implementations use the same derive with their extension trait,
for example `#[derive(ExtensionCall)] #[extension_call(DeviceExtension)]` on a
device type. For these traits the derive emits the registration hook instead of
RPC dispatch; the host still drives device DSP and panel drawing through the
dedicated ABI.

### Call-path diagram

```text
AUTHOR CALL                         GENERATED PROXY
PanelHandle::draw(&ctx, input)
  -> bitcode::serialize(PanelRequest::Draw { input })
  -> SharedRpcTransport::invoke(target, ctx, interface_id, version, "draw", payload)
                                      |
                                      |  host scheduler / RpcTransport
                                      |  resolves RemoteTarget and dispatches
                                      v
GUEST IMPLEMENTATION              GENERATED DISPATCHER
Box<dyn ExtensionCall>::process(&ctx, "draw", payload)
  -> bitcode::deserialize::<PanelRequest>(payload)
  -> <MyPanel as PanelDispatcher>::dispatch_extension(...)
  -> Panel::draw(&ctx, input)
  -> PanelResponse::Draw(Ok(frame))
  -> bitcode::serialize(response)
                                      |
                                      v
PanelHandle::draw decodes the matching response variant and returns
  Result<PanelFrame, ExtensionError>
```

Registration closes the loop:

```text
#[derive(ExtensionCall)] #[extension_call(Panel)]
  -> MyPanel: ExtensionCall { interface_id, version, process(...) }
register_extension_instance(POINT, MyPanel)
  -> Extension::typed_extensions[point_id].push(Box<dyn ExtensionCall>)
```

The actual implementation object stays inside the guest. Only stable ids,
method name, context, and bitcode bytes cross the scheduler boundary.

### Current integration status

The generated handle, dispatcher, `ExtensionCall` adapter, in-guest storage,
and `ExtensionContext::get_extension_handle` path are implemented and tested.
Handles are created directly from an `RpcTransport` or from a scheduler-backed
context. `get_extension_handle` currently uses the point id as the target
implementation id, so it is only suitable for single-provider points.

The package host does **not** yet export an invoke call, collect typed
implementations in the registration manifest, or wire the UI-thread scheduler
to native/WASM/browser instances. Package-to-package typed calls therefore
remain the next integration milestone. Host/core code that uses
`CoreExtensionRegistry` calls `Arc<T>` implementations directly; it does not
use generated handles.

## 5. Contribution contracts

### Devices

A device supplies a `DeviceSpec`, a processor instance, optional note handling,
latency, and a panel. `spec()` runs during registration; `describe` runs once per
declared device at host load. The engine and each visible panel use separate
instances. Prepare runs before processing and on format changes, note events
arrive in block order before `process`, and destroy runs on drop. The process
wire carries sample position and playing state, but the public
`DeviceExtension` API currently exposes neither.

`DeviceSpec::container` is optional metadata for a `DeviceKind::Container`.
The host owns the child chain; the guest supplies the stage, parameters, and
panel.

- `StageThenChildren` runs the stage, then children.
- `ChildrenThenStage` runs children, then the stage.
- `wet_param = Some(id)` makes the stage a wet path; the host applies dry/wet
  after children.
- If the stage cannot be created, children still process without the wet blend.
- Containers are serial only. Parallel/split and sidechain paths need a
  multi-port DSP ABI. Depth/cycle enforcement in child-chain metadata is still
  missing.

The stock `notwig.chain` keeps its dedicated core processor and does not use the
extension container path.

### Commands

A command has a stable descriptor, a bounded `CommandContext`, and a
`CommandEffect` containing at most `MAX_COMMAND_EDITS` host edits or an error.
The host validates the edit family and applies effects through the normal
command/undo path. Commands can appear in the palette and have bindings.
Commands cannot mutate `ProjectDoc` directly.

### Audio sub-editors

An audio sub-editor renders into the audio editor and returns a validated
`AudioSubEditorFrame` with a `CommandEffect` and optional clipboard points. The
context carries the clip snapshot, selection gestures, shared curve clipboard,
theme, and panel input. The host checks the document revision, applies an
`Apply` effect as one undo batch, and publishes returned clipboard points
through the normal curve clipboard.

### Bottom panels

`EP_NOTWIG_BOTTOM_PANEL` is a single point with many implementations. Core and
package panels are distinguished by stable panel ids; the shell keeps at most
one bottom panel open and selects it by id. A command opens one with
`CommandUiRequest::OpenBottomPanel`. Built-in editors use the same point with
ids such as `notwig.core.midi_editor_bottom_panel`.

## 6. Package ABI and hosts

`ABI_VERSION` is `9`. The host requires an exact match in the manifest
`api_version`, index entry, and the loaded module's `notwig_abi_version` export.
There is no negotiation or compatibility mode. Any export signature, wire
layout, or documented contract change requires a bump. Older versions added the
registration manifest (8), lifecycle phases (7), and container metadata (5);
version 9 added extension bottom panels.

| Export | Call point | Result |
| --- | --- | --- |
| `notwig_abi_version()` | module load | must equal `9` |
| `notwig_ext_preload()` | before load | `AbiStatus`; optional point declarations |
| `notwig_ext_load()` | after preload | `AbiStatus`; main contributions |
| `notwig_ext_registrations()` | after load, once | JSON registration manifest |
| `notwig_ext_unload()` | instance drop | `AbiStatus`; optional cleanup |
| `notwig_alloc` / `notwig_free` | WASM calls | bump-heap pointer |
| `notwig_device_describe(type)` | package load | serialized `DeviceSpec` |
| `notwig_device_create(type)` | engine build/panel draw | handle or failure |
| `notwig_device_prepare(handle, rate, max_block, channels)` | before processing | `AbiStatus` |
| `notwig_device_set_param(handle, id, tag, value)` | parameter edits | `AbiStatus` |
| `notwig_device_process(handle, audio, frames, events, count, position, playing)` | audio block | `AbiStatus` |
| `notwig_device_latency(handle)` | prepare | sample count |
| `notwig_device_draw(handle, context)` | panel frame | postcard `PanelFrame` |
| `notwig_audio_sub_editor_create/draw/destroy` | audio editor | postcard frames |
| `notwig_bottom_panel_create/draw/destroy` | bottom panel | postcard frames |
| `notwig_device_destroy(handle)` | drop | removes handle |

WASM modules may import only `notwig.host.log` and `notwig.host.notify`. Both
read guest linear memory, cap copied text at 8 KiB, and never call back into the
guest. WASM inputs/outputs use a fixed 1 MiB bump heap; pointers never move and
allocation fails on overflow. JSON results use a packed `(ptr, len)` in a
`u64`; native builds use a two-word struct. `AbiStatus` values are `Ok = 0`,
`Error = -1`, `BufferTooSmall = -2`, `UnknownDevice = -3`,
`InvalidArgument = -4`, and `FuelExhausted = -5`.

| Aspect | Native WASM (`wasmtime`) | Native library | Browser WASM |
| --- | --- | --- | --- |
| Trust | sandboxed | trusted, in-process | browser sandbox |
| Memory | 64 MiB store limit | process memory | browser-managed |
| Compute | 20M fuel per call | none | no fuel metering |
| Imports | `log`, `notify` | none; pointers | JS `log`, `notify` |
| Audio | copied through guest memory | processed in place | copied through typed arrays |
| Panels | postcard frame | postcard frame | postcard frame |

Discovery order never selects a winner: entrypoints are tried in declared order,
and target selection is recorded on the loaded extension. A candidate must
describe every declared device; otherwise the host tries the next supported
target. Point replacement edges, user-controlled package load order, and a full
resolution graph are not implemented yet.

## 7. Panels

Extensions never receive the host `egui::Context` or `Ui`. Each guest panel owns
a private `egui::Context`, runs the real egui API, tessellates, and returns a
validated `PanelFrame` with meshes, texture deltas, actions, cursor/repaint
hints, and input wants. The host composites the meshes into a clipped panel
surface and routes input back through a serializable snapshot.

```text
guest instance
  draw(&mut egui::Ui, context)
    -> private egui::Context::run
    -> tessellate
    -> PanelFrame { meshes, textures, actions, focus, repaint }
    -> postcard
host notwig-ext
  decode + validate
host notwig-ui
  upload/remap textures
  clip/translate meshes into the device card or panel
  apply parameter actions through normal validation
  request repaint / cursor
```

Validation limits include 8192 meshes, 262,144 vertices, 786,432 indices, 256
textures, 8192 px per texture side, 16M texels, 256 actions, 2048 input events,
64 KiB text, and 8 MiB serialized. Geometry must be finite, clip rects ordered,
indices in range and divisible by three, actions valid, and `TextureId::User`
rejected. Guest texture ids are remapped into a per-panel namespace. The host
keeps the last valid frame after a transient draw error.

Panels are clipped child surfaces. Floating guest windows, IME composition,
guest accessibility trees, arbitrary host widgets, host font sharing, and
overlay windows are not supported. Device panels and bottom panels use the same
frame path; audio sub-editors wrap it with clip context and command effects.

## 8. Clipboard

`EP_NOTWIG_CLIPBOARD` is a single-provider point. `ClipboardProvider` reads and
writes the structured `notwig_core::Clipboard` plus cached platform text. The
workspace installs a core provider by default and call sites use the point
instead of a UI enum. The provider is replaceable programmatically in the app;
package discovery and the runtime manifest do not yet expose clipboard
contributions, so a package cannot install a clipboard provider yet.

## 9. Threading and real-time rules

- Package discovery, resolution, registration, and panel drawing run on the UI
  or host load thread, never in the audio callback.
- Device processing runs on the audio path and must not allocate, block, perform
  I/O, load code, or run unbounded work. Setup belongs in construction/prepare.
- `DeviceStore` keeps instances behind individual locks. Calls to one handle
  serialize; different handles do not share a lock.
- Audio and panel instances are separate; a panel never shares mutable guest
  state with its processor.
- Generated typed handles are synchronous today. The package scheduler must not
  hold a registry lock while invoking guest code.
- The engine does not call generic extension RPC. Only compiled device
  processors enter the callback.

Generated ABI exports catch unwinding panics where possible: status calls
return `AbiStatus::Error`, pointer calls return null/empty, and destroy
swallows. Builds using `panic = "abort"` still abort. Extensions must not rely
on catching panics.

## 10. Testing

- `crates/notwig-ext/notwig-ext-api/tests/`: point/registry semantics, module
  registration, typed handle/dispatcher registration, lifecycle builders.
- `crates/notwig-ext/notwig-ext/tests/`: native WASM ABI, target selection,
  bundles, commands, sub-editors, bottom panels, and device packages.
- `crates/notwig-ext/notwig-ext-api/tests/panel_*.rs`: panel wire and bridge.
- `crates/notwig-ui/tests/extension_*.rs`: panel compositing, bottom-panel
  registry, and clipboard provider replacement.
- `crates/notwig-web` and `crates/notwig-ext/src/web/`: browser parity checks.
- `crates/notwig-ext-devices/tests/`: bundled device contracts and DSP.

## 11. Current gaps

- Package typed RPC: no invoke export, no typed manifest entries, no host
  scheduler, and no cross-package typed call test.
- Generated handles for many-cardinality points need an implementation id, not
  the point id. `get_extension_handle` is single-point only.
- `CoreExtensionRegistry` uses `Arc<T>` and returns host trait objects. It is not
  the package resolver and does not have generations or stale-handle checks.
- No replacement-edge graph, user-controlled package load order, or resolution
  diagnostics for conflicting providers.
- Package clipboard providers are not wired through the manifest/host.
- Browser host has no explicit memory or fuel cap; it relies on the browser
  sandbox.
- Panel IME, accessibility trees, overlay windows, host font sharing, and
  per-frame performance budgets remain open.
- Container child chains lack depth/cycle validation. Parallel/split, sidechain,
  and multi-port routing need a later DSP ABI.
- Device state blobs, device presets, and transport/tempo-aware device APIs are
  still missing.

## 12. Implementation map

`notwig-ext-api`:

- `src/extension_point.rs`: point keys and cardinality.
- `src/registry.rs`: `CoreExtensionRegistry` and `ExtensionContext`.
- `src/rpc.rs`: `ExtensionCall`, `ExtensionHandle`, `RpcTransport`,
  `RemoteTarget`.
- `src/lifecycle.rs`: phase builders and registration adapters.
- `src/extension.rs`: guest runtime, factories, per-instance stores.
- `src/registration.rs`: runtime manifest descriptors.
- `src/panel_frame/`: frame wire, limits, validation, egui bridge.
- `src/bottom_panel.rs`, `src/audio_editor.rs`, `src/command.rs`,
  `src/clipboard.rs`: point contracts.

`notwig-ext-macros`:

- `src/expand/load.rs`: lifecycle/contribution ABI generation.
- `src/trait_macro.rs`: typed trait/handle/dispatcher and `ExtensionCall`
  derive.

`notwig-ext`:

- `src/host.rs`, `src/compiled.rs`, `src/contributions.rs`: package loading and
  resolution.
- `src/abi/`: native WASM instance calls and memory.
- `src/native.rs`: native shared-library calls.
- `src/web/`: browser host, instances, panels, and bottom panels.

`notwig-ui` / `notwig-app`:

- `views/panel_registry.rs`: core bottom-panel descriptors.
- `views/shell/mod.rs`: active bottom panel and dispatch.
- `views/device_editor/panel_render.rs`: texture namespace, input, compositing.
- `workspace/clipboard_provider.rs`: core clipboard provider.
- `extensions/native.rs`, `extensions/web.rs`: host composition.
