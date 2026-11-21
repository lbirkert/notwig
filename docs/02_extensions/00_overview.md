# Extension Overview

Notwig extensions add devices. An extension can be built as a portable
WebAssembly module or as a native shared library for the current platform. Both
targets share the same author-facing `DeviceExtension` API. An extension can
provide:

- device metadata and parameters through a `DeviceSpec`;
- real-time DSP through the `DeviceExtension` trait;
- a custom immediate-mode panel drawn with Notwig's own UI API.

Extensions never receive `egui::Context`, host pointers to application state, or
the project document. The host sends a serialized input frame and receives
validated draw commands plus parameter actions.

## Architecture

```text
extension (wasm or native)      notwig-ext host              notwig-ui
──────────────────────────      ────────────────              ─────────
DeviceExtension::spec  ──────►  DeviceSpec validation  ───►  device chain
DeviceExtension::process ────►  audio buffers          ───►  engine
DeviceExtension::draw  ──────►  PanelDraw + actions    ───►  panels
```

- `notwig-ext-api` is the author-facing SDK. It has no `egui`, `wasmtime`,
  `libloading`, or I/O.
- `notwig-ext` is the host. It discovers packages, selects the first supported
  entrypoint, runs WASM with `wasmtime` on native or the browser `WebAssembly`
  API in the browser build, and loads native shared libraries with `libloading`
  on desktop. It enforces WASM resource limits and creates device instances.
  The browser build loads WASM entrypoints only, discovers them from extracted
  packages in OPFS, and installs packages from the same JSON index as the native
  app.
- `notwig-ui` replays the draw commands and applies validated parameter
  actions.

The extension ABI is versioned separately from the package manifest. The current
version is `2`. The WASM and native ABIs are different transport layers around
the same Rust API.

## What an extension can do

An extension can:

- add one or more audio effects or instruments to the device browser;
- read and write interleaved `f32` audio;
- declare continuous, boolean, enum, and discrete parameters;
- report processing latency;
- draw a custom panel using layout, widgets, primitives, text, and input;
- request a repaint or a cursor change.

A sandboxed WASM extension cannot:

- access the filesystem, network, threads, or wall clock;
- read or mutate the project document;
- load native plugins;
- create tabs, windows, or arbitrary host UI;
- allocate unbounded memory or run without fuel limits.

A native extension is not sandboxed and runs with the application's privileges.
Install native extensions only from trusted sources.

## Where to go next

1. [Quickstart](01_quickstart.md) builds and installs a working device.
2. [Package and Manifest](02_package_and_manifest.md) defines the installable
   layout and target selection.
3. [Device API](03_device_api.md) covers `DeviceExtension`, parameters, ports,
   DSP, and latency.
4. [Panel API](04_panel_api.md) covers layout, widgets, input, actions, and
   custom drawing.
5. [Testing and Distribution](05_testing_and_distribution.md) covers tests,
   the example limiter, gate, and synth, packaging, and index publishing.
