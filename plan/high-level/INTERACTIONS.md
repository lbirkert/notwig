# System Interactions (High-Level)

## 1. Systems Overview
- **Core (Domain):** Canonical project/document model and rules. Owns authoritative state, validates intents, and produces state changes and derived messages.
- **Project (Persistence):** Lifecycle and storage boundary. Loads/saves/migrates project bundles and assets; provides document/state to Core and persists accepted states.
- **Sequencer & Tracks (within Core):** Musical/time arrangement, track structure, routing intent, clips/notes/audio references as part of the document model.
- **DSP & Engine:** Realtime/offline audio execution. Builds and runs processing from prepared descriptions; owns live timing/meter/playback state and emits runtime events.
- **Plugins/Devices:** Processing units (stock FX and hosted instruments/plugins). Integrated as devices in chains/graphs with parameters, state, and reported capabilities/latency.
- **UI:** `egui`+`wgpu` presentation and interaction layer. Projects document/runtime/UI state to views, captures user intent, and issues commands/requests.
- **App (Composition):** Entry/wiring layer. Starts and connects subsystems, threads/workers, and communication channels per workspace/crate boundaries.

## 2. Ownership & Authority
- **Document truth (`S_doc`)**: owned and mutated only by Core.
- **Runtime truth (`S_rt`)**: owned by Engine (transport actuals, authoritative playback playhead, meters, status/latency) and observed by others. `S_doc.transport.playheadMus` is locate/last-saved position, not the live playhead.
- **Ephemeral UI state (`S_ui`)**: owned by UI (selection, focus, viewports, drags, panel layout).
- **Persistent artifacts**: project bundles/assets/caches managed by Project (and related non-RT services), referenced from `S_doc`.

## 3. Primary Interaction Flows
```text
User/UI
  -> (intent) Command
Core
  -> validate/apply to S_doc
  -> StateDelta/Snapshot/QueryResult to UI
  -> prepared EngineMsg to Engine (control boundary)
Engine (RT/non-RT split)
  -> executes audio/MIDI processing
  -> RtEvent/status/meters/playback updates to Core/UI
Project
  <-> load/save/migrate/recover bundle & assets
Plugins/Devices
  <-> instantiated/configured by Engine/host services from Core/DSP descriptions
```

## 4. Boundary Rules
- **UI ↔ Core:** UI never mutates `S_doc` directly; it sends commands and renders from projections/subscriptions.
- **Core ↔ Engine:** Core sends prepared, bounded messages; Engine applies at safe boundaries and never mutates `S_doc`.
- **Engine ↔ RT world:** Audio callback path is isolated from blocking/allocating/IO/compilation/UI work.
- **Project ↔ Core:** Persistence reads/writes validated representations; recovery/autosave restore consistent document states.
- **UI ↔ Project/Plugins (heavy work):** Scans, loads, compiles, imports, and exports are asynchronous/background from the UI frame path.

## 5. Typical Scenarios
- **Edit:** User action in UI -> Command -> Core updates `S_doc` and emits delta -> UI refreshes projection; Engine receives necessary updates.
- **Playback:** Transport command -> Core/Engine coordination -> Engine advances `S_rt` and produces audio/MIDI -> UI displays playhead/meters from runtime events.
- **Device/param change:** UI edits parameter -> Command -> Core validates/records -> Engine receives parameter/update -> processing reflects change.
- **Open/save:** UI/App requests -> Project loads/saves bundle -> Core receives/serves validated document -> UI and Engine synchronize to new state.

## 6. Scope Note
- This is a structural map for navigation and reasoning. Precise schemas, invariants, and contracts are defined in `OVERVIEW.md` and subsystem docs under `plan/crates/notwig-*/` and `plan/code/`.
- v0 is playback/edit only: no recording, live input capture/monitoring, or metronome; MIDI clips carry notes only; Instrument tracks host plugin instruments.
