# notwig-ext

WASM extension host for Notwig. This crate owns the runtime side of the
extension system; extension authors depend on `notwig-ext-api` instead.

## Responsibilities

- Discover extensions from a user-provided directory and validate manifests.
- Compile and cache core wasm modules with `wasmtime` (Cranelift JIT).
- Enforce per-extension trust levels: `Trusted` (no fuel), `Limited` (fuel +
  smaller memory), and `DrawOnly` (no DSP).
- Describe extension devices and register their `DeviceSpec`s.
- Create WASM-backed `DeviceProcessor`s through `ExtensionDeviceFactory`.
- Host extension device panels through `ExtensionPanelHost`, translating the
  returned `PanelDraw` into UI commands.
- Provide process-wide ABI helpers: allocation, JSON snapshots, packed
  pointer/length results, and memory-bounds checks.

## Boundaries

- No `egui` and no UI code. Panel output is render-neutral
  `notwig_ext_api::PanelDraw`.
- No project mutation. Extensions see snapshots and return draw/audio data; the
  app validates and applies state changes.
- No filesystem, network, or threads inside extension modules. The ABI is core
  wasm without WASI.
- The audio callback never compiles or instantiates modules. DSP instances are
  created on the engine build thread and only call prepared exports.

## Module map

| Module | Responsibility |
|---|---|
| `abi.rs` | wasmtime engines/stores, host imports, memory helpers, typed exports |
| `compiled.rs` | loaded extension records and type-indexed lookup |
| `device.rs` | `ExtensionDeviceFactory`, WASM `DeviceProcessor`, draw-only silence |
| `discovery.rs` | user-directory scan, manifest parse/validate, module size checks |
| `error.rs` | `ExtensionError` |
| `host.rs` | `ExtensionHost`: discovery + compile + specs + factory + panel host |
| `panel.rs` | UI-side instance cache implementing `DevicePanelHost` |

## ABI overview

Required exports: `notwig_abi_version`, `notwig_alloc`, `notwig_free`, and the
`notwig_device_*` family (`describe`, `create`, `destroy`, `prepare`,
`set_param`, `process`, `latency`, `draw`). Metadata, parameter values, and
panel draw output cross the boundary as JSON; audio crosses as interleaved
`f32` samples in extension linear memory.

Host imports are limited to `notwig.host.log` and `notwig.host.notify`.

## Trust levels

| Level | DSP | UI | Limits |
|---|---|---|---|
| `Trusted` | yes | yes | 64 MiB, no fuel |
| `Limited` | yes | yes | 32 MiB, per-call fuel |
| `DrawOnly` | no | yes | 16 MiB, per-call fuel |

`DrawOnly` devices still appear in the device chain but process silence.

## Testing

`tests/abi.rs` builds tiny WAT modules inline with the `wat` crate, so the test
suite does not require a wasm toolchain. It covers device description, panel
draw output, audio processing, dynamic latency, and draw-only fallback.
