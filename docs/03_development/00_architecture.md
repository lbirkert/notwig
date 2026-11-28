# Architecture

Notwig is a Rust workspace with one application binary (`notwig`) and a set of
libraries. The architecture favors a small number of explicit boundaries:
domain state is separate from runtime audio state, UI code emits intents
instead of mutating documents directly, and the real-time engine owns playback.

## Layers

```text
notwig-app
  |  composition, workspace policy, audio I/O, history, dialogs
  v
notwig-ui          notwig-engine
  |  egui views      |  real-time execution
  v                  v
notwig-core::dsp <-- notwig-plugins
  |  device plans    |  hosted instruments/effects
  v                  v
notwig-core  <-----  notwig-project
   domain, protocol     bundles, assets, audio import/export
```

Dependency direction is inward:

- `notwig-core` has no workspace dependencies.
- `notwig-project` depends on core.
- Device contracts, plan compilation, and stock devices live in
  `notwig-core::dsp`; core stays free of I/O, UI, and real-time execution.
- `notwig-plugins` depends on core (including DSP contracts).
- `notwig-engine` depends on core and plugins.
- `notwig-ui` depends on core, project, and the extension API; it does not
  depend on engine internals. Its dependency-light `components` module is shared
  with extension devices.
- `notwig-app` composes everything and owns process-level concerns.

### Crate dependency graph

The edges below come from the workspace `Cargo.toml` files. An edge means the
crate names the other crate in `[dependencies]`; feature-gated, optional, and
target-specific edges are marked.

```text
notwig-web
  └─> notwig-app                         (wasm32 target; default-features = false)
        ├─> notwig-ui
        │     ├─> notwig-core
        │     ├─> notwig-project         (optional, "app" feature) -> notwig-core
        │     └─> notwig-ext-api         (optional, "app" feature) -> notwig-core,
        │                                                          notwig-ext-macros
        ├─> notwig-project -> notwig-core
        ├─> notwig-engine
        │     ├─> notwig-core
        │     └─> notwig-plugins -> notwig-core
        │                        └─> notwig-rack   (optional, separate workspace)
        ├─> notwig-plugins -> notwig-core
        ├─> notwig-ext
        │     ├─> notwig-core
        │     └─> notwig-ext-api -> notwig-core, notwig-ext-macros
        ├─> notwig-ext-api -> notwig-core, notwig-ext-macros
        └─> notwig-core

notwig-ext-devices -> notwig-core, notwig-ext-api, notwig-ui (default-features = false)
cargo-notwig-ext   -> notwig-ext-api
```

Edges in detail:

| Crate | Workspace dependencies | Notes |
| --- | --- | --- |
| `notwig-core` | none | Lowest layer. |
| `notwig-project` | `notwig-core` | Bundle and asset I/O. |
| `notwig-ext-macros` | none | Proc-macro crate. |
| `notwig-ext-api` | `notwig-core`, `notwig-ext-macros` | Shared by hosts and guest devices. |
| `notwig-ext` | `notwig-core`, `notwig-ext-api` | `native` (wasmtime/libloading) and `web` features. |
| `notwig-ext-devices` | `notwig-core`, `notwig-ext-api`, `notwig-ui` | All without default features. |
| `cargo-notwig-ext` | `notwig-ext-api` | Scaffolding and packaging. |
| `notwig-engine` | `notwig-core`, `notwig-plugins` | Real-time execution. |
| `notwig-plugins` | `notwig-core`; optional `notwig-rack` | Format adapters are feature-gated. |
| `notwig-ui` | `notwig-core`; optional `notwig-project`, `notwig-ext-api` | The `app` feature enables the optional edges. |
| `notwig-app` | `notwig-core`, `notwig-project`, `notwig-ext-api`, `notwig-ext`, `notwig-engine`, `notwig-plugins`, `notwig-ui` | Native and wasm targets add their own platform dependencies. |
| `notwig-web` | `notwig-app` | Browser entry point. |
| `notwig-rack` | none (separate workspace) | Vendored plugin wrappers consumed by `notwig-plugins`. |

The graph must stay acyclic. Shared boundary types belong in the lowest crate
that needs them; most cross-layer types live in `notwig-core`.

## State model

```text
S_doc   canonical project document (serializable, undoable)
S_rt    engine-owned transport, meters, playback state (not persisted)
S_ui    ephemeral view state (selection, drags, open panels, drafts)
```

`ProjectDoc` is the single source of truth for tracks, clips, devices, assets,
transport locates, and mixer/device parameter values. UI views are projections
of `S_doc` plus `S_rt` observations.

The engine owns the authoritative playhead during playback. The document stores
the locate/last-saved playhead. Stop returns to the play start captured when
playback began; looping does not change that return position.

## Data flow

### Editing

```text
pointer gesture
  -> UI selection / edit operation
  -> notwig-core::edit algebra (pure plan)
  -> notwig-app::edit_lowering (IDs + CommandPayload)
  -> CommandPayload
  -> notwig_core::apply
  -> new ProjectDoc + EngineMsg / RtEvent
  -> Workspace updates document and engine
```

The UI never mutates `ProjectDoc` directly. `notwig-app::Workspace` applies
commands, records undo history, and decides whether the engine needs a full
rebuild or a lightweight synchronization message. See
[`Command Application and Undo`](13_command_application_and_undo.md) for the
apply, batch, delta, and history contracts, and
[`UI Contracts and Coordinates`](17_ui_contracts_and_coordinates.md) for the
view/intent boundary.

### Engine builds

Structural changes (tracks, devices, plan topology, sample rate) start an
asynchronous engine build:

1. `Workspace` snapshots the document, device registry, plugin descriptors, and
   audio assets into an `EngineBuildRequest`.
2. A background thread compiles the DSP plan and constructs a new `Engine`.
3. The UI thread polls the result. If more changes arrived meanwhile, exactly
   one follow-up build is queued.
4. The new engine is installed under the engine mutex; the old engine is
   dropped outside the lock. Transport position is preserved.

Mixer/parameter changes use `sync_engine`, which updates track state, sends
parameters, and refreshes the playback schedule without rebuilding the plan.

### Audio output

`notwig-app::audio::AudioService` owns the CPAL output stream. In the common
high-block configuration the `Engine` runs on a dedicated render worker and the
callback consumes pre-rendered rings; when project and device blocks are equal,
the engine runs directly in the device callback. In both cases the audio thread
does not share a mutex with the UI. Instead the app publishes control data
through the exchange described in [`Audio Engine`](02_audio_engine.md):

- the app owns a `Box<EngineData>` and modifies it without locks;
- `EngineControl::flush` swaps it into `Mutex<(Option<Box<EngineData>>, bool)>`;
- the audio thread tries the slot once per fixed frame interval, swaps its own
  control box, and applies the new data;
- a retired box, runtime, plan, and asset map return to the app thread, so the
  audio thread does not free them.

Normal control updates apply in place and keep DSP state. Structural changes
arrive as a prebuilt `PreparedRuntime` produced by the engine build/plugin-loader
thread. The audio thread only moves the prepared value into place; it never
compiles a plan, instantiates a plugin, or allocates buffers.

The engine renders in project blocks and updates playhead, meters, and DSP timing
in an `AudioSnapshot`. The UI reads snapshots through a short mutex lock; the
render thread uses `try_lock` and is never blocked by the UI. If the control slot
is contended, the render thread skips that sync point and retries at the next
interval.

The engine chunks oversized device blocks to `max_block_size` so a large host
block cannot violate device expectations. Stop triggers all-notes-off and
renders the tail with clip playback disabled.

The real-time invariant is no allocation, blocking locks, file I/O, plugin
loading, or unbounded work in the audio callback. Control exchange and
prepared-runtime swaps are designed to preserve that invariant.

### Project persistence

A project is a directory bundle:

```text
project.notwig/
  project.json       manifest + document
  assets/            imported audio files
  peaks/             reserved for waveform data
  autosave/          snapshot-<revision>-<timestamp>.json
  state/             persisted plugin state blobs
  cache/             reserved for derived data
```

`notwig-project` validates the schema major version, normalizes volatile fields
(playing/recording), and reports missing assets as load issues. Hosted plugin
state blobs are written under `state/`; `peaks/` and `cache/` remain reserved.
The app creates
`session.lock` when a bundle is opened or saved and removes it on clean exit.
When a lock and autosave snapshots are both present, the app offers to recover
the latest autosave or discard it before continuing. See
[`Persistence and Recovery`](14_persistence_and_recovery.md) for the manifest,
schema, atomic-write, autosave-selection, and recovery behaviour.

User settings are stored separately from projects. Shortcut bindings and the
audio driver, device, and block size live in per-user config files; the sample
rate stays project-scoped.

### Browser target

`notwig-web` compiles the application to `wasm32-unknown-unknown` and starts the
same `notwig-ui`/`notwig-engine` composition through `eframe::WebRunner`.
Platform edges are selected by target:

- project storage uses a `Projects` folder in OPFS instead of `std::fs`, with an
  in-app project picker and `.notwig.zip` download;
- audio uses CPAL's WebAudio backend and starts after a user gesture;
- native plugin formats are not compiled;
- extensions use the browser WebAssembly host instead of wasmtime or native
  loading; only `wasm` entrypoints are eligible, the same JSON index is fetched
  with `fetch`, and installed packages are extracted into
  `notwig-extensions/<id>/` in OPFS and rediscovered on startup;
- the engine build runs synchronously on the UI thread in the first browser
  milestone.

The browser bundle is static output. It needs no server headers and is deployed
under `/app/` beside the mdBook documentation. See
[`Web Build and Storage`](20_web_build_and_storage.md) for the OPFS layout,
storage keys, gesture gate, and deployment assumptions.

## Crates at a glance

| Crate | Owns | Must not |
|---|---|---|
| `notwig-core` | Domain types, IDs, time, tracks, sequencer, command protocol, edit algebra, DSP contracts/plans/stock devices | I/O, UI, real-time execution, ID allocation |
| `notwig-project` | Bundle read/write, autosave/recovery, audio probe/import/decode/resample/export | UI, real-time processing |
| `notwig-ext-api` | Extension SDK, guest panel wire types/runner, note-event wire types, manifests, targets, ABI helpers | `wasmtime`, `libloading`, I/O, canonical state |
| `notwig-ext-macros` | `#[notwig_ext(preload/load/unload)]` proc macro that generates WASM and native exports | Runtime host behavior |
| `notwig-ext` | Extension discovery, target selection, native wasmtime/WASM and shared-library hosts, device DSP, panel host | UI layout, canonical state, audio-callback compilation |
| `notwig-ext-devices` | Example and stock extension devices built against the SDK and shared `notwig-ui` components | Host behavior or app UI code |
| `cargo-notwig-ext` | Scaffolding and target-specific extension ZIP packaging | UI or runtime host behavior |
| `notwig-engine` | Real-time playback, plan application, scheduling, transport, meters | Mutating `ProjectDoc`, blocking the audio callback |
| `notwig-plugins` | Plugin discovery/cache, host adapters, instance/state/editor lifecycle | Leaking format-specific types across the crate boundary |
| `notwig-ui` | `egui` shell, views, editors, interaction, intents, runtime projection, shared extension components, guest panel compositing | Canonical state, engine internals, blocking work |
| `notwig-app` | Composition, workspace policy, audio I/O, thread wiring, dialogs, history | Reusable domain/DSP logic |
| `notwig-web` | Browser entry point, wasm-bindgen handle, WebRunner startup | UI/domain/DSP behavior |
| `notwig-rack` | Vendored VST3/VST2/CLAP/LV2/AU/AU3 wrapper crates outside the workspace | Notwig domain or UI behavior |

## Threads and processes

- **UI process/thread**: `eframe`/`egui` event loop, canonical `ProjectDoc`,
  workspace mutations, dialogs, plugin catalog, and extension panels.
- **Engine process**: native child process started with `notwig engine-host`.
  It owns CPAL, the `Engine`, plan/runtime construction, native plugin DSP,
  extension devices, and native plugin editor windows. It receives serialized
  build/control messages and returns snapshots and editor events.
- **Device callback thread (inside the engine process)**: CPAL-owned real-time
  callback. It consumes high/low rings in high-block mode, or renders inline
  when the project and device blocks match.
- **Render worker (inside the engine process)**: owns the engine in high-block
  mode and fills the project and low-latency monitor rings.
- **Engine build thread (inside the engine process)**: runtime construction and
  plugin instantiation.
- **Plugin scan process**: separate `notwig scan-plugins <cache>` process, with
  plugin metadata cached for the next launch.
- **Plugin load probe**: `PluginHost` runs `notwig load-plugin` in a child
  process before loading a plugin in the app, so a crashing plugin fails
  metadata loading instead of taking down the UI or engine process.
- **Plugin GUI**: native plugin editor windows are owned by the engine process
  on macOS. The UI process controls them over IPC.
- **Web**: the browser build keeps the in-process engine; it has no native
  plugins or OS process isolation.

The engine process handshake, message families, build queue, prepared-runtime
swap, and restart policy are specified in
[`Engine Process and Supervision`](15_engine_process_and_supervision.md). The
plugin adapter and editor contract is in
[`Plugin Hosting`](16_plugin_hosting.md). Startup and shutdown ordering is in
[`Application Lifecycle`](18_app_lifecycle.md).

## Error handling

- Domain and project boundaries return typed errors (`CoreError`,
  `ProjectError`, `EngineError`, `PluginError`).
- Recoverable user/input failures are returned as `Result`; they do not panic.
- The real-time path reports problems through `RtEvent`/snapshot status instead
  of unwinding across the callback boundary.
- The workspace records the latest error/status for the UI to display.

## Extensibility points

- **Commands**: add a `CommandPayload` variant, implement its application in
  `notwig-core::apply`, and lower it from the app/UI.
- **Devices**: add a `DeviceSpec`/processor in `notwig-core::dsp`, register it,
  then add UI metadata if the generic parameter view is not enough.
- **Editors**: implement an `EditModel` adapter and a lowering path; reuse the
  shared algebra and clipboard.
- **Panels/windows**: keep view code in `notwig-ui`, emit `UiIntent` values, and
  implement the behavior in `notwig-app::Workspace` or `gui`.
- **Plugin formats**: implement a `HostAdapter` in `notwig-plugins`; keep
  format-specific types inside that crate.
- **Extensions**: add host behavior in `crates/notwig-ext/notwig-ext` and
  shared ABI/panel types in `notwig-ext-api`. Source manifests omit
  `entrypoints`; `cargo notwig-ext` adds the built target entrypoint and filters
  `includes`. Device specs, DSP, and panels come from the extension;
  Notwig owns validation, resource limits, and rendering. Panels run a private
  guest egui context and send validated meshes/texture deltas for the host to
  composite. The point/registry model, typed call path, lifecycle, ABI, hosts,
  panels, and current gaps are all documented in
  [`Extension System`](19_extension_system.md). General clip/track drawing hooks
  remain future work.
- **Keyboard shortcuts**: add a command spec in `notwig-ui::commands` and
  handle it in the focused view. Bindings are edited in Settings and persisted
  by `notwig-app`.
