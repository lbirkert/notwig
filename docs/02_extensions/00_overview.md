# Extension Overview

Notwig extensions add devices and commands. An extension can be built as a
portable WebAssembly module or as a native shared library for the current
platform. Both targets share the same author-facing API. An extension can
provide:

- device metadata and parameters through a `DeviceSpec`;
- real-time DSP through the `DeviceExtension` trait;
- a custom panel drawn with the guest egui API and shared `notwig-ui`
  components;
- commands with manifest metadata, a bounded `CommandContext`, validated edit
  families, command palette entries, and default keybindings.

Extensions never receive the host `egui::Context`, host pointers to application
state, or the project document. The extension runs a private egui context and
returns validated tessellated meshes, texture deltas, and parameter actions.

## Architecture

```text
extension (wasm or native)      notwig-ext host              notwig-ui
──────────────────────────      ────────────────              ─────────
DeviceExtension::spec  ──────►  DeviceSpec validation  ───►  device chain
DeviceExtension::process ────►  audio buffers          ───►  engine
DeviceExtension::draw ──►  PanelFrame + actions    ───►  panels
```

- `notwig-ext-api` is the author-facing SDK. It has no `wasmtime`, `libloading`,
  or I/O. It exposes the guest egui runner and wire types.
- `notwig-ext` is the host. It discovers packages, selects the first supported
  entrypoint, runs WASM with `wasmtime` on native or the browser `WebAssembly`
  API in the browser build, and loads native shared libraries with `libloading`
  on desktop. It enforces WASM resource limits and creates device instances.
  The browser build loads WASM entrypoints only, discovers them from extracted
  packages in OPFS, and installs packages from the same JSON index as the native
  app.
- `notwig-ui` composites the returned panel frame and applies validated
  parameter actions. Extension panels use its shared knobs, meters, labels,
  sections, toggles, and dropdowns.
- `cargo notwig-ext` scaffolds extensions and builds target-specific ZIPs. It
  reads a source `extension.json` without `entrypoints`, adds the selected
  entrypoint, filters `includes` by target, removes `includes`, and bundles the
  artifact, manifest, icon, and extra files.

The extension ABI is versioned separately from the package manifest. The current
ABI version is `5`. The manifest layout is versioned separately: device-only
packages may omit `manifest_version`, while command contributions require
version `2`. The WASM and native ABIs are different transport layers around the
same Rust API.

## What an extension can do

An extension can:

- add one or more audio effects or instruments to the device browser;
- add commands that read the current selection and return validated edits, such
  as humanizing MIDI notes;
- read and write interleaved `f32` audio;
- declare continuous, boolean, enum, and discrete parameters;
- report processing latency;
- declare a serial container device with a host-owned child chain, including a
  wet FX chain;
- draw a custom panel with the real egui API inside its own context;
- use shared `notwig-ui` controls and the shared color palette;
- request a repaint or a cursor change.

A sandboxed WASM extension cannot:

- access the filesystem, network, threads, or wall clock;
- read or mutate the project document;
- load native plugins;
- create host tabs, windows, or arbitrary host UI;
- mutate the project document directly from a command.

The native wasmtime host enforces a 64 MiB memory limit and fuel per call. The
browser host runs inside the browser sandbox but does not add an explicit
memory or fuel cap, so those limits are not available there.

A native extension is not sandboxed and runs with the application's privileges.
Install native extensions only from trusted sources.

## Where to go next

1. [Quickstart](01_quickstart.md) builds and installs a working device.
2. [Package and Manifest](02_package_and_manifest.md) defines the installable
   layout and target selection.
3. [Device API](03_device_api.md) covers `DeviceExtension`, parameters, ports,
   DSP, and latency.
4. [Panel API](04_panel_api.md) covers the guest panel, shared components,
   input, and actions.
5. [Testing and Distribution](05_testing_and_distribution.md) covers tests,
   the bundled compressor, clipper, delay, chorus, transient, dynamics, reverb,
   synth, and gain devices, packaging, and index publishing.
6. [Command API](06_command_api.md) covers manifest v2 command contributions,
   the bounded context, host edit families, the palette, and keybindings.
