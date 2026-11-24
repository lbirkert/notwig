# Engine Process and Supervision

The native desktop app runs the audio engine in a child process. The UI owns
`ProjectDoc`, undo history, persistence, and the plugin catalog; the `engine-host`
child owns CPAL, the engine, plugin DSP instances, and native editor windows.
Related: [`Audio Engine`](02_audio_engine.md),
[`Plugin Hosting`](16_plugin_hosting.md), [`Architecture`](00_architecture.md),
and [`Logging and Diagnostics`](22_logging.md).

## 1. Process boundary

`EngineClient::start` spawns the current executable with the hidden `engine-host`
argument. Stdin/stdout are piped; stderr is inherited. The child initializes its
own `engine-<timestamp>-<pid>.log` in the Notwig app directory before entering
the host loop. The child runs an IPC reader, an IPC writer, an `EngineWorker`
control thread, a loader thread, and a main loop that owns editor windows
(main-thread AppKit requirement).

```text
UI process                                  engine-host child
Workspace / ProjectDoc                      IPC reader (stdin)
  │  compile plan on build thread               │
  │  WireBuild ─────── length-prefixed ────────►│
  │                                             ▼
  │                                        EngineWorker control loop
  │                                          ├─ AudioService / CPAL
  │                                          ├─ Engine + plugin DSP
  │                                          └─ loader thread (prepare)
  │◄──── snapshots, build/editor/recording ─────┤ IPC writer (stdout)
  │                                             │
EngineClient reader thread                 main loop: EditorHost windows
```

## 2. Framing and handshake

Frames are a 4-byte little-endian length followed by a `postcard` payload, with a
1 GiB limit. Truncated input at a frame boundary is a clean EOF; other read errors
become IPC errors. The parent sends `Hello { version: PROTOCOL_VERSION }`, requires
`Ready` with the same version, then sends `OpenDevice { project_rate, settings }`
and waits for `DeviceOpened` or `Error`. `PROTOCOL_VERSION` is 1. A version
mismatch is rejected with `Error`; the parent kills the child on a rejected
handshake or an unexpected device reply. A read failure before the async threads
start aborts startup and drops the pipes. `DeviceOpened` carries the actual driver,
device name, sample rate, channel count, and block size. The parent then starts
`notwig-engine-ipc-writer` and `notwig-engine-ipc-reader`.

## 3. Message families

Requests are `EngineRequest`; child-to-UI values are `EngineReply`. `WireBuild`
carries engine config, compiled plan, schedule, asset map, plugin descriptors and
states, and extension root/settings.

| Family | Direction | Ownership and reply |
| --- | --- | --- |
| `Hello`, `OpenDevice` | UI → child | UI owns settings; child owns CPAL. Replies: `Ready`, `DeviceOpened`, `Error`. |
| `Build(WireBuild)` | UI → child | UI owns the document and plan; child owns instantiated processors and the runtime. Reply: `BuildFinished`. |
| `Control(EngineControlUpdate)` | UI → child | UI owns policy; child owns `EngineData` and live DSP state. No direct reply; effects appear in snapshots. |
| `ConfigureInputs` | UI → child | Child owns `InputHub` routes. Failure replies `InputError`; success has no reply. |
| `StartRecording`, `StopRecording` | UI → child | Child owns `RecordingSession`. Replies: `RecordingChunk`, then `RecordingFinished` or `RecordingFailed`. |
| `OpenEditor`, `CloseEditor`, `FocusEditor`, `SetEditorParam` | UI → child | Child's main thread owns windows and a separate editor plugin instance. Replies: `EditorOpened`, `EditorClosed { state, error }`, `EditorParam`, `EditorTransportToggle`. |
| `Snapshot` | child → UI | Child owns the live snapshot; UI keeps a timestamped copy. Sent about every 16 ms. |
| `RtEvent` | internal child | Not a frame. `Meters`, `Status`, and `Error` are folded into snapshots; latency and transport events stay internal. Child errors also appear in `Snapshot.error`. |

Snapshots carry `playhead_age_nanos`, measured when the worker serialized them, so
the UI timestamps the audio position rather than the delayed IPC arrival.

## 4. Worker lifecycle and build policy

`OpenDevice` bumps the build generation, drops pending and queued builds, stops
recording silently, drops the service and input routes, and clears control data.
A build without a device fails immediately with `BuildFinished(Err(...))`.

Both build stages keep at most one follow-up request:

```text
idle ──Build──► building ──Build──► queued (replace any older queued build)
                   │
                   ├─ done, current, nothing queued ──► install runtime, reply Ok
                   ├─ done, superseded ──────────────► discard result, no reply
                   └─ done or disconnected ──────────► start queued if present
```

The UI-side `EngineProcessBuild` thread compiles the plan and schedule into
`WireBuild` and never loads plugin code. The child `notwig-engine-loader` thread
instantiates devices and builds the `ActivePlan`. A later request replaces the
single queued request; after the in-flight build finishes, exactly one follow-up
starts. Superseded results are neither installed nor reported. Control, input,
recording, and snapshots continue during a build. `BuildFinished(Ok)` means the
runtime reached the child control slot, not that the audio callback has swapped
it yet.

## 5. Prepared-runtime swap

`Engine::prepare_runtime` builds the `ActivePlan` on the loader thread. The worker
installs it with `set_prepared_runtime` and flushes the local `EngineControl`
exchange. At the next sync (every 256 frames by default) the audio thread moves it
into place, replaces plan/schedule/assets, emits `RtEvent::LatencyChanged`, and
re-primes notes covering the current playhead. Retired runtime values return
through the exchange and are dropped by the worker. A pending prepared runtime
survives later non-prepared flushes; `EngineControl::flush` carries it, and any
unconsumed transport, forward. The UI never receives or owns the runtime.

## 6. Crash detection, restart, and backoff

A callback panic is caught by `catch_unwind` in the child audio path, which sets
`snapshot.crashed` and an error; the process stays alive but stops rendering. A
hard exit is detected by the parent reader as EOF or a read error, which marks
`running = false`, `crashed = true`, and sends `EngineEvent::Error`.

The UI reports a crash once and does not automatically restart it. The user
restarts through `UiIntent::RestartAudioEngine`: the old `EngineClient` drops
(sends `Shutdown`, then kill/wait), a new child starts with the current rate and
settings, transport is republished at the last observed playhead, and a rebuild is
requested. Automatic backoff applies to device loss, not a generic crash. When the
output disappears, the app pauses transport/recording if needed and retries after
500 ms, doubling to a 5 s cap, only while the configured output resolves. A failed
crash-restart with an unavailable output enters that path.

## 7. State across restart

| State | After restart |
| --- | --- |
| `ProjectDoc`, undo history, dirty/bundle state | Preserved; the UI is authoritative. |
| Assets and plugin state blobs | Preserved in the UI and resent in the next build. |
| Last observed playhead | Republished as the new transport position. |
| Child DSP runtime, plugin instances, delay lines, voices, previews | Dropped and rebuilt. |
| Meters, xruns, DSP history, low-latency monitor chains | Reset. |
| Child recording session and unforwarded capture tail | Dropped. |
| Native editor windows and live editor state | Dropped; only previously saved blobs survive. |

Normal shutdown sends `Shutdown` and kills the child. The child reader forwards
`WorkerCommand::Shutdown` and `EditorCommand::Shutdown`; the worker finishes the
recording session silently without sending `RecordingFinished`, and editors close.
Because the parent kills immediately after `Shutdown`, that teardown races on
restart. Application shutdown closes editors first and waits up to two seconds for
`EditorClosed` states. A crash or restart does not synthesize `RecordingFinished`:
forwarded chunks stay in the UI take, the child tail is lost, and recording does
not resume.

## 8. Browser difference

The wasm build compiles out `engine_process`. `restart_audio` starts the
in-process `AudioService` with the workspace control handle; `EngineBuild`
prepares runtimes locally. There is no `engine-host`, IPC, subprocess plugin scan,
native plugin host, or native editor. The runtime and control rules are otherwise
the same.

## 9. Invariants

1. Plugin loading and runtime construction never run in the audio callback.
2. Build completion never blocks control, input, recording, or snapshot handling.
3. The UI document is authoritative after a crash; the child is disposable.
