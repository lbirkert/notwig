# notwig-ext

Extension discovery, loading, and runtime hosting.

## Purpose

`notwig-ext` scans the user extensions directory (and browser packages), loads
WASM or trusted native extension modules, caches compiled modules, and exposes
device specs, DSP processors, and panel hosts to the app. It never renders UI
and never depends on `notwig-ui`.

## Owns

- Manifest discovery and target selection.
- WASM module compilation/cache and native shared-library loading.
- Extension ABI calls: describe, create, prepare, set parameter, process, note
  events, latency, and panel frames.
- Trust/limits, budgets, and host imports.
- `ExtensionPanelHost`, which owns one UI-side extension instance per device and
  returns a validated `PanelFrame` for the UI to composite.

## Module map

| Module | Responsibility |
| --- | --- |
| `abi.rs` | Native WASM host facade: engine setup and re-exports |
| `abi/instance.rs` | WASM instantiation, preload/load/unload exports, registration manifest, and per-call fuel reset |
| `abi/memory.rs` | Guest linear-memory reads/writes and packed result decoding |
| `abi/host.rs` | `notwig.host.log/notify` imports with bounded text copies |
| `abi/status.rs` | `AbiStatus` decoding and parameter wire conversion |
| `native.rs` | Native shared-library loading and raw export calls |
| `web/instance/mod.rs` | Browser WASM instance and export lookups |
| `web/instance/host.rs` | JS host object and bounded `log/notify` glue |
| `web/instance/memory.rs` | Browser typed-array memory access |
| `web/instance/abi.rs` | Browser exported ABI calls |
| `web/instance/status.rs` | Browser status, parameter, and JS error conversion |

## Does not own

- Device UI drawing or texture/mesh compositing (see `notwig-ui`).
- Project state or command application (see `notwig-app`/`notwig-core`).

See [`docs/02_extensions/`](../../../docs/02_extensions/) for authoring and
packaging.
