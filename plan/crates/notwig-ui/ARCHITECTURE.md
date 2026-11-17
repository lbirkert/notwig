# UI Architecture (`notwig-ui`)

## 1. Technology & Scope
- `egui` (immediate-mode UI) + `wgpu` renderer.
- UI is a client of domain/engine state; MUST NOT own canonical `S_doc` mutations.
- v0 excludes Clip Launcher/scenes, recording/live-input UI, stock instrument editors, metronome/count-in, undo/redo UI, collaboration presence, and modulation/macro editors.
- Included shell surfaces: transport, arranger, mixer, browser, editors, settings/preferences, notifications/dialogs (missing assets, plugin errors, xruns).

## 2. State Model
- `S_doc`: canonical (Core-owned). `S_rt`: engine-owned runtime (playhead, transport actuals, meters, xrun/status, latency). `S_ui`: UI-owned ephemeral (selection, focus, hover, viewport/zoom/scroll, panel sizes, drag state, overlays).
- `Frame = π(S_doc, S_rt, S_ui)` rendered each update; `π` side-effect free w.r.t. `S_doc`.

## 3. Data Flow
- `UI -> Core`: `Command` intents only (validated/applied by Core).
- `Core -> UI`: `Snapshot|StateDelta|QueryResult` ordered by revision.
- `Engine -> UI/Core`: `RtEvent` stream/snapshots for `S_rt` via bounded/non-blocking channels.
- Consistency: semantic document visuals from `S_doc`; timing/meters from `S_rt`. UI tolerates refresh skew but never invents semantic state.

## 4. Rendering & Performance
- High-DPI correctness and smooth interaction (pan/zoom/drag/edits).
- Custom paint/wgpu primitives allowed for dense visuals (waveforms, spectra, EQ curves, meters, timelines) behind reusable components.
- No blocking on UI frame: IO, plugin scan/load, DSP compile, decode/peak generation run off-thread with async results/progress.
- Large/dynamic data (peaks LOD, analyzer frames) consumed incrementally by viewport.

## 5. Decomposition
- `AppShell` (transport, workspace, side panels, status), `View` (projection + emit commands/local `S_ui`), `Primitive` (reusable component), `DeviceView` (device composition over primitives/bindings).

## 6. Clean-room
- Bitwig-inspired density/workflow as reference; original identity and implementation. No copied code/assets from Bitwig or Hikaru OpenLive.
