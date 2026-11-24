# Application lifecycle

Status: describes the current native application path. The browser target uses
the same views and workspace but substitutes web storage and an in-process audio
service. See [`Architecture`](00_architecture.md), [`Audio Engine`](02_audio_engine.md),
and [`Code Structure`](01_code_structure.md).

## 1. Startup order in `NotwigApp::new`

`gui::run` installs the wgpu viewport and constructs `NotwigApp`. Startup then
runs these steps in order:

| Step | Action | Current behaviour |
| --- | --- | --- |
| 1 | Open the initial bundle | `Workspace::open` loads the bundle, assets, plugin states, and calls `begin_session`; `None` uses `Workspace::new_empty`. An open error falls back to an empty workspace with a status message. |
| 2 | Load recovery candidates | Only when no bundle was requested. `load_startup_recovery_candidates` reads the registry and sets `recovery_offered`; the modal appears later in `update`. |
| 3 | Validate audio settings | Enumerates drivers/devices, then `audio_config::validated` drops unavailable selections. Workspace block size and low-latency monitoring are reconciled and a rebuild is requested if they changed. |
| 4 | Start background workers | Two long-lived threads: audio import and audio analysis, each with request/result channels. |
| 5 | Load extensions | `ExtensionManager::load` loads settings and the native extension host on the UI thread. |
| 6 | Construct UI state | Builds `NotwigUi`, loads shortcuts, starts the 1.5 s wordmark splash timer, and retains the document playhead as the fallback playhead. |
| 7 | Synchronise extensions | `sync_extensions` installs device specs, the engine extension runtime, and the panel host. Installing specs requests an engine rebuild while no audio process exists yet. |
| 8 | Start audio | `restart_audio` starts the engine process, performs the handshake and device open, installs the remote control sink, republishes transport, requests another rebuild, and configures recording inputs. |

The double rebuild at steps 7 and 8 is observable: the first request targets the
local engine build, the second targets the engine process. If audio start fails,
`audio_start_failed` either reports the error or enters device-lost recovery and
schedules a retry.

## 2. Per-frame update order

`eframe::App::update` is single-threaded and ordered as follows:

| Order | Work | Notes |
| --- | --- | --- |
| 1 | Poll engine build | Delivers a prepared `WireBuild` to the engine process, or installs a local `PreparedRuntime`. |
| 2 | Drain native async jobs | Native dialogs, audio analysis results, audio imports; wasm drains encoding separately. |
| 3 | Advance render | Renders up to eight blocks of an active mixdown per frame. |
| 4 | Drain render jobs | Native encode and stem threads; wasm polls web encoding and recording. |
| 5 | Poll plugin catalog | Refreshes descriptors and may request a rebuild if the project uses plugins. |
| 6 | Poll extensions | Handles loaded/installed/error events, calls `sync_extensions` when needed, and requests 100 ms repaints while busy. |
| 7 | Drain engine events | Build results, plugin editors, parameter edits, transport toggles, and recording chunks. |
| 8 | Take audio snapshot | Updates the stored playhead, checks device loss, reports crashes, and advances the fallback playhead when the engine is not running. |
| 9 | Refresh derived UI data | Recording waveforms and smoothed meters. |
| 10 | Build runtime view and show UI | Registers device specs, then `ui.show` returns `UiIntent`s that `handle_intent` processes. The startup wordmark overlay is painted above the normal UI while its fade window is active. |
| 11 | Show recovery modal | Runs after intents and before autosave. |
| 12 | Save shortcuts and autosave | Autosave is due after 60 s of dirty state, with a bundle path, and no pending audio import. |
| 13 | Schedule repaint | Fast while rendering or playing; slower while idle or while the engine builds. |

## 3. Intent delegation

Views never mutate `ProjectDoc`. `NotwigUi::show` returns `UiIntent` values.
`NotwigApp::handle_intent` delegates to `Workspace` methods or calls
`Workspace::apply` with a single payload. It also owns policy that is not
document state: dialog spawning, recording capture, audio restarts, selection
updates after create/duplicate, and shortcut persistence. `UiIntent::Quit` is
translated to a viewport close request. Selection-only intents are handled by UI
state.

## 4. Workspace policy boundaries

- `notwig-core` owns the document, command semantics, validation, edit algebra,
  and engine control message shapes. It does not own I/O, threads, or history.
- `Workspace` owns project lifecycle, apply/undo/redo, dirty and autosave state,
  asset loading, session locks, plugin state storage, engine build requests, and
  engine publishing. It still does not run UI gestures or blocking dialogs.
- `NotwigApp` owns process and platform edges: the engine child process, audio
  device restart policy, extension manager, dialogs, worker channels, and the
  eframe frame loop.
- UI lowering allocates command IDs in the app layer; core currently has
  implicit ID-allocation exceptions documented in
  [`Command Application and Undo`](13_command_application_and_undo.md).

## 5. Async job lifecycle and cancellation gaps

- Audio import and analysis threads live for the whole session. They are not
  joined, cannot be cancelled, and deliver results to whatever workspace exists
  when a result arrives. Pending import directories are removed by
  `Workspace::drop`.
- Engine builds use a generation counter and keep at most one queued request.
  A superseded build is discarded when it finishes, but the running build thread
  is not cancelled. The engine process repeats this pattern in its own worker.
- Extension index fetches and installs run on detached threads. Installs are
  serialised by `install_rx`, but nothing cancels a running fetch or install;
  a new index fetch can replace the old receiver while its thread still runs.
- Mixdown rendering is advanced cooperatively from `update`; stem and encode
  jobs use detached threads. Opening or creating a project does not clear an
  active render job, and these threads are not joined.
- Native file dialogs run on a detached thread and cannot be cancelled when the
  window closes.
- Autosave failures are logged and surfaced; the deadline is not advanced. It is
  still skipped while audio imports are pending.

## 6. Shutdown and session end

Shutdown has two phases:

1. `on_exit` calls `Workspace::end_session`, which removes `session.lock` for a
   bundle, then saves audio settings. It does not force a final autosave.
2. `Drop for NotwigApp` closes open plugin editors, polls for their state for up
   to two seconds, and stores returned plugin state. `EngineClient::drop` then
   sends `Shutdown`, kills the child, and waits for it.

Fields drop in declaration order, so `Workspace` drops before the `EngineClient`
field; `Workspace::drop` only removes temporary import directories. Background
threads are not joined explicitly, so process exit reclaims them. An active
recording is not explicitly stopped or finalised on exit.

Replacing a project follows a related path: opening a different bundle or
creating a new project calls `end_session` on the old workspace before
installing the new one. Discarding recovery also removes the lock and calls
`begin_session` again.
