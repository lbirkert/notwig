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
  events, latency, and egui panel frames.
- Trust/limits, budgets, and host imports.
- `ExtensionPanelHost`, which owns one UI-side extension instance per device and
  returns a validated `EguiFrame` for the UI to composite.

## Does not own

- Device UI drawing or texture/mesh compositing (see `notwig-ui`).
- Project state or command application (see `notwig-app`/`notwig-core`).

See [`docs/02_extensions/`](../../docs/02_extensions/) for authoring and
packaging.
