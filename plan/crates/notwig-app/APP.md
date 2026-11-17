# App Composition & Runtime (`notwig-app`)

## 1. Responsibilities
- Binary entrypoint and platform/bootstrap (windowing, `wgpu`/`egui` integration, audio backend hookup; MIDI input is out of v0).
- Wire subsystems: Core (`S_doc`, command handling), Project (load/save/autosave/assets), Engine (RT/offline), Plugins (scan/host services), UI (views/state).
- Own global configuration/settings, logging/tracing setup (non-RT), and lifecycle orchestration.

## 2. Runtime / Thread Topology (v0 target)
- **UI thread:** event loop, `S_ui`, rendering, user input -> dispatch `Command`/requests (non-blocking); also pumps main-thread-required plugin editors.
- **Core/control context:** apply commands to `S_doc`, emit `StateDelta` and `EngineMsg`, coordinate project operations (may be UI-thread driven or dedicated; mutations serialized).
- **Audio RT thread/callback:** engine `process` only; communicates via bounded lock-free queues (`EngineMsg` in, `RtEvent` out).
- **Background workers:** project IO/autosave/import/export, plugin scan/load, DSP compilation, asset decode/resample/peak build; results delivered to control/UI via channels.

## 3. Channel Plumbing
- UI -> Core: commands/queries. Core -> UI: snapshots/deltas/results.
- Core -> Engine: prepared `EngineMsg`. Engine -> UI/Core: `RtEvent`.
- Project/Plugins/DSP services <-> control/UI: async request/response or progress events.

Queues crossing RT boundary MUST be bounded and RT-safe.

## 4. Startup/Shutdown
- Startup: init config/logging -> create services/workers -> load default/new/requested project (determines project rate) -> open audio device at project rate with configured buffer -> init UI -> sync initial state/plan.
- Opening/creating a project with a different sample rate closes/reopens the audio device before playback; failure leaves the previous project state intact and reports an error.
- Audio device hotplug/loss: pause processing, reprepare/rebuild, resume; surface explicit status. No device => silent operation with UI notice.
- Shutdown: stop playback/RT -> flush/save prompts/autosave as applicable -> release plugins/engine/devices -> persist caches as policy -> exit cleanly.

## 5. Global Configuration (v0)
- Location: platform config/cache dirs (macOS: `~/Library/Application Support/Notwig/`, `~/Library/Caches/Notwig/`; XDG equivalents when ported). Not stored in the project bundle.
- Format: versioned TOML with sections:
  - `audio: { backend, outputDevice, sampleRatePref, bufferFramesPref }` (`sampleRatePref` seeds new projects and requests the device rate; open projects require the device at `Meta.sampleRate`)
  - `midi: { inputPorts, outputPorts }` (reserved/inert in v0: no live input)
  - `plugins: { scanPaths, formats: { vst3, au, vst2 } }`
  - `ui: { theme, scale, layout }`
  - `project: { recent, autosaveIntervalSecs, defaultTemplate }`
  - `logging: { level, file }`
- Defaults with no config: system-default audio output, 48 kHz, 256-frame buffer, standard VST3/AU locations, dark theme, 60 s autosave interval.
- Config migrations are versioned; unknown keys are ignored deterministically and preserved where possible.
- The UI renders a settings snapshot and emits typed `SettingsIntent`s (`SetAudioDevice`, `SetBufferFrames`, `SetSampleRate`, `SetPluginScanPaths`, `SetTheme`, `SetUiScale`, `OpenRecent`, `ClearRecent`); the app validates and persists them. UI types are view-only and `notwig-ui` does not depend on `notwig-app`.
- v0 has no project-level device overrides.

## 6. Platform & Scope
- macOS first (VST3 + AU; VST2 fallback), portable architecture for later Windows/Linux; audio/plugin backends behind traits.
- v0 single-process, in-process plugins, playback/edit only (no recording, live input, metronome, or stock instrument), no collaboration/undo runtime.
- App composes; crate-specific contracts live in respective `plan/crates/<crate>/` docs.
