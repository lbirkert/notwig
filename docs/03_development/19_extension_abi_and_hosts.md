# Extension ABI and hosts

Status: implemented. This page records the extension package/runtime split, ABI
version policy, host boundary, device call order, transport differences, and
known gaps. Author-facing usage is in [`docs/02_extensions`](../02_extensions/00_overview.md).

Related: [`Extension Panels`](12_extension_panels.md),
[`UI Contracts and Coordinates`](17_ui_contracts_and_coordinates.md), and
[`crates/notwig-ext`](../../crates/notwig-ext).

## 1. Package and runtime architecture

An installed extension is a directory with `extension.json`, one or more
entrypoint files, and an optional icon. The manifest declares ordered
`entrypoints`, each naming a relative file and a target. `notwig-ext-api` is
the author SDK and holds the shared wire types; `notwig-ext-macros` generates
the exported functions around the `#[notwig_extension]` entrypoint.

`notwig-ext` discovers packages and rejects unsupported API versions, duplicate
device type IDs, unsafe paths, and oversized entrypoints. For each declared
device it calls `notwig_device_describe` and requires the returned
`DeviceSpec::type_id` to match the manifest. Device specs from all loaded
packages are registered as stock-like device types in the app.

## 2. ABI version and target selection

`ABI_VERSION` is `6`. The host requires an exact match in three places: the
manifest `api_version`, the index entry `api_version`, and the value returned
by the loaded module's `notwig_abi_version` export. Any change to an export
signature, wire layout, or documented contract must bump the constant; there is
no negotiation or backwards-compatible mode. Old packages then fail to load
instead of being misread. Version 5 added the optional `DeviceSpec::container`
metadata described in section 4.

Manifest entrypoints are tried in order and the first supported target wins.
WASM is supported on every host (wasmtime natively, `WebAssembly` in the
browser). Native shared libraries are supported only when the operating system
and architecture match the build. The browser loader requires a WASM
entrypoint. Entrypoint paths must stay inside the package and are capped at
64 MiB.

## 3. Host boundary and memory

WASM modules may import exactly two host functions under `notwig.host`:

- `log(level: i32, ptr: i32, len: i32)`: level `0` is debug, `1` is info, and
  every other value is a warning.
- `notify(ptr: i32, len: i32)`: currently logged at info level; there is no
  separate notification queue.

Both read guest linear memory, ignore invalid pointer/length pairs, and decode
text lossily. Each call copies at most 8 KiB of guest text, reports whether the
message was truncated, and skips decoding entirely when no tracing subscriber
enables the requested level. A valid message therefore allocates nothing on the
audio path unless it is actually logged. They never call back into the guest.

```text
host                                  guest module
─────────────────────────────         ─────────────────────────────
describe/create/prepare/...  ───────► exported notwig_device_* functions
set_param/process/draw       ───────►   │
                                    │ uses abi::alloc / free
                                    │ uses DeviceStore handles
                                    ▼
log/notify imports           ◄─────── notwig.host.log / notify
```

WASM inputs and outputs use a fixed bump heap (`notwig_alloc`,
`notwig_free`) of 128 Ki u64 words (1 MiB). Pointers never move, allocation
returns `-1` on overflow, and `notwig_free` only releases the most recent
allocation when the pointer matches; audio, event, and static JSON buffers are
effectively retained. A JSON result is a packed `(ptr, len)` in a `u64`; native
builds return a two-word `{ptr, len}` struct. `AbiStatus` is `Ok = 0`,
`Error = -1`, `BufferTooSmall = -2`, `UnknownDevice = -3`,
`InvalidArgument = -4`, and `FuelExhausted = -5`.

## 4. Device lifecycle and export sequence

`spec()` runs once when the guest registry initializes; describe runs once per
declared device at host load. One instance serves each engine device and a
separate one serves each visible panel keyed by `device_id`. Prepare runs
before processing and on format changes, note events arrive in block order
before `process`, `latency` is queried during prepare, and destroy runs on drop.

| Export | Call point | Result |
| --- | --- | --- |
| `notwig_abi_version()` | module load / instantiate | must equal `5` |
| `notwig_alloc` / `notwig_free` | WASM only, before/after host inputs | bump-heap pointer |
| `notwig_device_describe(type)` | package load, per manifest device | serialized `DeviceSpec` |
| `notwig_device_create(type)` | engine build or first panel draw | positive handle or negative failure |
| `notwig_device_prepare(handle, rate, max_block, channels)` | before processing / format change | `AbiStatus`; generated wrapper currently ignores `max_block` |
| `notwig_device_set_param(handle, id, tag, value)` | host edits and panel replay | `AbiStatus`; `tag` is ignored and the value is passed as `f64` |
| `notwig_device_process(handle, audio, frames, events, count, position, playing)` | audio block | `AbiStatus`; generated wrapper currently ignores `position` and `playing` |
| `notwig_device_latency(handle)` | prepare | sample count |
| `notwig_device_draw(handle, context)` | panel frame | postcard `PanelFrame` |
| `notwig_audio_sub_editor_create(editor_id)` | first extension sub-editor frame | positive handle or negative failure |
| `notwig_audio_sub_editor_draw(handle, context)` | extension sub-editor frame | postcard `AudioSubEditorFrame` |
| `notwig_audio_sub_editor_destroy(handle)` | sub-editor close / instance drop | removes the handle |
| `notwig_device_destroy(handle)` | instance drop | removes the handle |

### Containers

`DeviceSpec::container` is optional metadata for a `DeviceKind::Container`
device. The host owns the child chain; the guest supplies only the stage,
parameters, and panel. This needs no child-processor ABI and no guest callback.

- `routing = StageThenChildren` (default) runs the stage, then the child chain.
- `routing = ChildrenThenStage` runs the child chain, then the stage.
- `wet_param = Some(id)` turns the container into a wet path. The host copies
  the input to a dry buffer, runs the stage and children as the wet path, and
  crossfades with `dry + (wet - dry) * mix`. The stage must output wet only;
  the host applies the mix. If the stage cannot be created the child chain is
  still processed and no wet blend is applied.

The stock `notwig.chain` keeps its dedicated `ChainProcessor` and does not use
this path.

## 5. Threading and reentrancy

Audio processing and panel drawing use different extension instances, so a
panel never shares guest state with its processor. `DeviceStore` keeps each
instance behind its own lock: calls to one handle serialize, while different
handles do not wait on each other. Calls are synchronous and not reentrant; the
only host imports are `log` and `notify`, which never invoke guest code. WASM
audio is copied into guest memory and back; native audio is processed in place.
The guest `process` must still avoid blocking, allocation, locking, and I/O.

## 6. Native, WASM, and browser hosts

| Aspect | Native WASM (wasmtime) | Native library | Browser WASM |
| --- | --- | --- | --- |
| Trust | sandboxed | trusted, in-process | browser sandbox |
| Memory | 64 MiB `StoreLimits` | process memory | no explicit cap configured |
| Compute limit | 20,000,000 fuel per call, reset per call | none | no fuel metering |
| Host imports | `notwig.host.log/notify` | none; process pointers | JS `log/notify` to console |
| Audio path | copied through guest memory | in-place host pointer | copied through JS typed arrays |
| Panels | postcard frame | postcard frame | postcard frame |

The browser host loads only WASM through `js_sys::WebAssembly`, enforces the
same frame validation, and returns the same `PanelFrame` wire data, but its
sandbox limits are whatever the browser provides.

## 7. Panel frame wire contract

`DeviceDrawContext` is postcard-encoded and contains the device identity,
parameters, theme, meter snapshot, and a `PanelInput` snapshot. The guest runs
a private `egui::Context`, tessellates, and returns `PanelFrame`:
primitives, texture deltas, freed texture ids, actions, cursor, keyboard and
pointer wants, optional copied text, and a repaint delay.

The host validates the frame before use: at most 8192 meshes, 262,144 vertices,
786,432 indices, 256 textures, 8192 px per texture side, 16M texels, 256
actions, 2048 input events, 64 KiB text, and 8 MiB serialized. Geometry must be
finite, clip rects ordered, indices in range and a multiple of three, and
actions must pass parameter validation. `TextureId::User` is rejected; managed
guest texture ids are remapped into a per-panel namespace. The last valid frame
is reused after a transient draw error. `wants_keyboard` suppresses host
shortcuts on the following frame.

Custom audio sub-editors use the same `PanelFrame` payload wrapped in an
`AudioSubEditorFrame` with a `CommandEffect`. The host validates both the panel
and the edit budget. The context is an `AudioSubEditorContext` snapshot keyed by
clip and contribution id; the guest never receives the project document. The
host checks the captured document revision before applying an edit, and applies
an `Apply` effect as one undo batch.

## 8. Known gaps and documentation drift

- The WASM-offset helpers (`with_str`, `with_bytes`, `read_param_id`,
  `with_audio_buffer`, `with_audio_frames`) take a safe `i32` offset only
  because the guest supplies an address inside its own linear memory. They
  validate null/length, size, and alignment but cannot verify provenance, so
  callers must pass guest-managed offsets. The native raw-pointer helpers
  (`with_str_ptr`, `with_bytes_ptr`, `read_param_id_ptr`,
  `with_audio_frames_ptr`) and `Extension::process_ptr` are `unsafe fn` with
  explicit `# Safety` contracts.
- The generated ABI wraps every export that can recover in `catch_unwind`:
  status-returning calls return `AbiStatus::Error`, pointer-returning calls
  return null/empty, and `destroy` swallows the panic. This catches unwinding
  panics on native builds and on WASM builds that unwind. Builds compiled with
  `panic = "abort"` still abort, and an unwinding panic cannot cross the
  `extern "C"` boundary, so extensions must continue to avoid panics.
- Only native wasmtime enforces 64 MiB and 20M fuel per call. The browser glue
  caps `log`/`notify` copies at 8 KiB, but the browser host configures no memory
  cap and no fuel metering, so it relies on the browser sandbox.
- The process wire carries `sample_position` and `playing`, and prepare carries
  `max_block`, but the generated wrappers currently discard them. The public
  `DeviceExtension` API does not promise these values, so the code is the
  narrower contract.
- Containers are serial only. Guests cannot see, reorder, or route child
  devices, and there is no parallel/split or sidechain path. Multi-port routing
  needs a later ABI.
- The engine does not yet enforce a maximum container depth or detect cycles in
  child-chain metadata; malformed projects should be validated before adding
  arbitrary extension-created topology.
