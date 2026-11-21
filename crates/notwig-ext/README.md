# notwig-ext

Extension host for Notwig. This crate owns the runtime side of the extension
system; extension authors depend on `notwig-ext-api` instead.

## Responsibilities

- Discover extensions from a user-provided directory, validate manifests, and
  select the first supported entrypoint in manifest order.
- Validate and extract ZIP packages through a shared archive reader with path,
  size, and manifest checks used by both native installs and the browser.
- Compile core wasm modules with `wasmtime` (Cranelift JIT), run them through
  the browser `WebAssembly` API on the web, or load native shared libraries with
  `libloading` on desktop.
- Always run WASM with a memory limit and per-call fuel metering. Native
  libraries run trusted inside the process; there is no native sandbox.
- Describe extension devices and register their `DeviceSpec`s.
- Create WASM- or native-backed `DeviceProcessor`s through
  `ExtensionDeviceFactory`.
- Host extension device panels through `ExtensionPanelHost`, translating the
  returned `PanelDraw` into UI commands.
- Provide ABI helpers: allocation, JSON snapshots, pointer/length results,
  native pointer validation, and memory-bounds checks.

## Boundaries

- No `egui` and no UI code. Panel output is render-neutral
  `notwig_ext_api::PanelDraw`.
- No project mutation. Extensions see snapshots and return draw/audio data; the
  app validates and applies state changes.
- WASM modules get no filesystem, network, threads, or WASI imports. The native
  loader is explicitly trusted and deliberately not sandboxed.
- The audio callback never compiles, loads, or instantiates modules. DSP
  instances are created on the engine build thread and only call prepared
  exports.

## Module map

| Module | Responsibility |
|---|---|
| `abi.rs` | wasmtime engines/stores, host imports, memory helpers, typed exports |
| `archive.rs` | shared ZIP/file-tree validation and extraction with safety limits |
| `compiled.rs` | loaded extension records, runtime kind, and type-indexed lookup |
| `native.rs` | `libloading` loader and native shared-library instance |
| `instance.rs` | WASM/native instance enum with common device calls |
| `device.rs` | `ExtensionDeviceFactory`, WASM and native `DeviceProcessor`s |
| `discovery.rs` | user-directory scan, manifest parse/validate, first supported entrypoint |
| `error.rs` | `ExtensionError` |
| `host.rs` | `ExtensionHost`: discovery + compile + specs + factory + panel host |
| `install.rs` | filesystem install/replace and uninstall using validated archives |
| `panel.rs` | UI-side instance cache implementing `DevicePanelHost` |
| `web/` | Browser WebAssembly host, ZIP package parsing, DSP, and panels |

## ABI overview

The WASM ABI requires `notwig_abi_version`, `notwig_alloc`, `notwig_free`, and
the `notwig_device_*` family (`describe`, `create`, `destroy`, `prepare`,
`set_param`, `process`, `latency`, `draw`). Metadata, parameter values, and
panel draw output cross the boundary as JSON; audio crosses as interleaved
`f32` samples and note events cross as fixed-size `NoteEvent` records passed to
`process` when the device declares a note input. WASM host imports are limited
to `notwig.host.log` and `notwig.host.notify`.

The native ABI exports the same function names with pointer-sized arguments.
Inputs are ordinary process pointers, stored JSON outputs are returned as a
two-word pointer/length struct, and `process` receives the host audio buffer
directly. There is no WASM heap or copy on the native path.

## Sandboxing

| Target | DSP | UI | Limits |
|---|---|---|---|
| WASM | yes | yes | 64 MiB memory, per-call fuel |
| Native | yes | yes | none; runs trusted |

## Testing

`tests/abi.rs` builds tiny WAT modules inline with the `wat` crate, so the test
suite does not require a wasm toolchain. It covers device description, panel
draw output, audio processing, dynamic latency, and ABI mismatch handling. The
`limiter_extension` test also loads the built WASM module and native shared
library when present.
