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
notwig-dsp  <-----  notwig-plugins
  |  device plans    |  hosted instruments/effects
  v                  v
notwig-core  <-----  notwig-project
   domain, protocol     bundles, assets, audio import/export
```

Dependency direction is inward:

- `notwig-core` has no workspace dependencies.
- `notwig-project` depends on core.
- `notwig-dsp` depends on core.
- `notwig-plugins` depends on core and DSP contracts.
- `notwig-engine` depends on core, DSP, and plugins.
- `notwig-ui` depends on core, project, DSP, and the extension API; it does not
  depend on engine internals.
- `notwig-app` composes everything and owns process-level concerns.

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
the locate/last-saved playhead. Stop behavior is defined by the transport
contract (loop start when looping, otherwise zero).

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
rebuild or a lightweight synchronization message.

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

`notwig-app::audio::AudioService` owns the CPAL output stream and the
audio-thread `Engine`. The audio thread does not share a mutex with the UI.
Instead the app publishes control data through the exchange described in
[`Audio Engine`](02_audio_engine.md):

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

The callback then processes one block and updates playhead, meters, and DSP
timing in an `AudioSnapshot`. The UI reads snapshots through a short mutex lock; the audio thread uses `try_lock` and never
blocks the audio thread. If the control slot is contended, the audio thread
skips that sync point and retries at the next interval.

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
the latest autosave or discard it before continuing.

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
under `/app/` beside the mdBook documentation.

## Crates at a glance

| Crate | Owns | Must not |
|---|---|---|
| `notwig-core` | Domain types, IDs, time, tracks, sequencer, command protocol, edit algebra | I/O, UI, real-time processing, ID allocation |
| `notwig-project` | Bundle read/write, autosave/recovery, audio probe/import/decode/resample/export | UI, real-time processing |
| `notwig-dsp` | Device/parameter contracts, registry, factories, graph compilation, stock devices, editor monitoring | Audio callback, persisted instance state |
| `notwig-ext-api` | Extension SDK, render-neutral draw/panel commands, note-event wire types, manifests, targets, ABI helpers | `egui`, `wasmtime`, `libloading`, I/O, canonical state |
| `notwig-ext-macros` | `#[notwig_extension]` proc macro that generates WASM and native exports | Runtime host behavior |
| `notwig-ext` | Extension discovery, target selection, native wasmtime/WASM and shared-library hosts, device DSP, panel host | UI layout, canonical state, audio-callback compilation |
| `notwig-ext-devices` | Example and stock extension devices built against the SDK | Host behavior or UI code |
| `notwig-engine` | Real-time playback, plan application, scheduling, transport, meters | Mutating `ProjectDoc`, blocking the audio callback |
| `notwig-plugins` | Plugin discovery/cache, host adapters, instance/state/editor lifecycle | Leaking format-specific types across the crate boundary |
| `notwig-ui` | `egui` shell, views, editors, interaction, intents, runtime projection | Canonical state, engine internals, blocking work |
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
- **Engine thread (inside the engine process)**: CPAL-owned real-time callback.
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
- **Devices**: add a `DeviceSpec`/processor in `notwig-dsp`, register it, then
  add UI metadata if the generic parameter view is not enough.
- **Editors**: implement an `EditModel` adapter and a lowering path; reuse the
  shared algebra and clipboard.
- **Panels/windows**: keep view code in `notwig-ui`, emit `UiIntent` values, and
  implement the behavior in `notwig-app::Workspace` or `gui`.
- **Plugin formats**: implement a `HostAdapter` in `notwig-plugins`; keep
  format-specific types inside that crate.
- **Extensions**: add host behavior in `notwig-ext` and shared ABI/draw types in
  `notwig-ext-api`. Device specs/panels/DSP come from the extension; Notwig owns
  validation, resource limits, and rendering. General clip/track drawing hooks are
  planned on the same command IR.
- **Keyboard shortcuts**: add a command spec in `notwig-ui::commands` and
  handle it in the focused view. Bindings are edited in Settings and persisted
  by `notwig-app`.
