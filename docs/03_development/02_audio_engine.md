# Audio engine control exchange

Status: implemented.

Related: [`Architecture`](00_architecture.md),
[`Code Structure`](01_code_structure.md), and the
`crates/notwig-engine/README.md`.

## 1. Principle

The audio thread must not build, load, allocate, or take a blocking lock while
processing. Control changes originate on the app/UI thread, and structural work
(plan compilation, plugin instantiation, buffer allocation) happens on loader
threads. The audio thread only swaps already-prepared values into place.

The design has two goals:

1. **No allocation on the audio thread.** The app allocates and prepares; the
   audio thread swaps pointers.
2. **No blocking on the audio thread.** The audio thread uses `try_lock` and
   skips a sync point when the app is publishing.

A retired value returns to the app thread, so freeing also happens off the audio
thread.

## 2. Exchange shape

```text
app side                                      audio side
Box<EngineData> engine_data                   Box<EngineData> control_data
        |                                             ^
        | flush                                       | periodic sync
        v                                             |
Mutex<(Option<Box<EngineData>>, updated: bool)> ------+
```

On the native desktop app the exchange runs inside the **engine process**:
`EngineControl` lives in the child, `ControlHandle` is attached to the child's
audio engine, and the parent UI publishes serialized `EngineControlUpdate`
messages instead of touching the shared box directly. The browser and offline
render paths keep the in-process exchange below.

The shared slot is a mutex around an optional box and an `updated` flag.
`EngineControl` owns the app-side box. `ControlHandle` is the shared handle the
audio engine reads.

### App publish

1. Modify `engine_data` freely without locking.
2. Lock the slot.
3. Exchange the app box with the slot box.
4. If the outgoing box has no prepared runtime but the box still in the slot
   does, move the pending `PreparedRuntime` into the outgoing box first. A
   non-prepared publish can therefore never discard a runtime the audio thread
   has not consumed yet.
5. Set `updated = true`.
6. Unlock and keep the returned (previous) box.
7. Clear the returned box on the app thread so it can be reused.

The returned box may contain the audio thread's retired runtime, plan, schedule,
or scratch buffers. Dropping and clearing it on the app thread keeps frees off
the audio thread. Two prepared publishes replace each other; the newest wins and
the older runtime is dropped on the app thread.

### Audio sync

At a fixed frame interval:

1. `try_lock` the shared slot. On contention, skip this sync point.
2. If `updated` is false, do nothing.
3. Take the published box.
4. Put the audio thread's previous box back into the slot.
5. Set `updated = false` and unlock.
6. Apply the new control data. Move any prepared runtime into place and put the
   previous runtime, plan, schedule, and asset map into the box that will be
   returned to the app.

The swap itself is pointer moves. No box, buffer, map, or plan is allocated or
freed while the callback holds the audio data.

## 3. Native process boundary

```text
UI process                                  engine process
Workspace / EngineControl remote sink  ───►  IPC request writer
        │                                       │
        │  EngineClient                         ▼
        │                                  EngineControl -> local exchange -> audio thread
        │                                       │
        ◄──── snapshots / editor events ────────┘
```

- The UI process sends `Build`, `Control`, and editor commands over a
  length-prefixed `postcard` protocol on the child's stdin/stdout.
- The engine process owns CPAL, the engine, plugin DSP instances, extension
  devices, and plugin editor windows.
- The UI process receives `AudioSnapshot` updates, build results, editor
  parameter changes, and editor close/state events.
- If the child exits, the UI marks the audio snapshot crashed and shows the
  restart overlay. Restarting rebuilds the child from the canonical
  `ProjectDoc` in the UI process.
- Web keeps the in-process exchange because browsers cannot spawn OS processes
  and do not host native plugins.

## 4. Cadence

The audio thread accumulates processed frames and checks the slot once per
`DEFAULT_CONTROL_SYNC_FRAMES` (256 frames by default). The interval is measured
in audio frames, not wall-clock time, so it is deterministic and independent of
scheduler jitter. `Engine::set_control_sync_frames` overrides it.

A sync point is skipped, not retried immediately, if the slot is contended. The
next interval tries again.

## 5. `EngineData`

`EngineData` is the app-owned control payload. It carries:

- the latest `PlaybackSchedule`;
- track state updates (mute, solo, gain, pan);
- parameter updates;
- an optional one-shot transport update;
- an optional metronome-only update;
- an optional all-notes-off request;
- an optional full asset map;
- an optional `PreparedRuntime` for structural changes.

Normal mixer, parameter, schedule, and asset updates omit `PreparedRuntime`, so
the existing DSP runtime and its delay lines, active notes, and processor state
stay in place. A metronome-only update also omits the transport, so toggling the
click never moves the playhead or clears sustained notes.

The runtime measures each device's input and output peaks around `process` and
keeps them in an `ActivePlan` map. `Engine::device_meter_snapshot` exposes that
map to the app so extension panels can render meters without processing audio in
the UI thread.

### Transport is one-shot

Transport updates carry a playhead target and are only applied when a transport
command staged one. A mixer or parameter publish must not rewind the playhead,
so ordinary control snapshots do not set `transport`.

### Runtime swaps and seeks re-prime notes

A new `ActivePlan` has no active-note state. After installing a prepared runtime,
and after a transport update that actually changes the playhead, the engine asks
the runtime to prime notes that cover the current playhead: the next block emits
note-ons at offset zero for those notes. Unchanged transport publishes do not
re-prime already-active notes.

### Assets are swapped as a map

Asset updates carry the full asset map as an `Arc<BTreeMap<...>>`. The audio
thread swaps the `Arc`, so importing or relinking audio does not rebuild the DSP
plan or load plugins.

## 6. Structural changes and the plugin loader

Structural changes (new tracks, devices, routing, plugin instances, tempo, sample
rate) need new processors and buffers. They are never built on the audio thread.

`notwig-engine::RuntimeLoader` spawns the plugin-loader thread. It:

1. compiles the `CompiledPlan`;
2. instantiates stock and hosted processors;
3. allocates track buffers and delay lines;
4. builds an `ActivePlan`;
5. packages a `PreparedRuntime`.

The app publishes the `PreparedRuntime` inside `EngineData`. The audio thread
only moves it into place. The previous runtime goes back through the exchange to
the app thread for disposal.

This separation also means a slow plugin load delays a rebuild, but never blocks
the audio callback.

## 7. Ownership summary

| Value | Built on | Consumed on | Freed on |
| --- | --- | --- | --- |
| `EngineData` box | app | audio sync | app (returned box) |
| control vectors/maps | app | audio sync | app (returned box) |
| `ActivePlan` / processors | loader thread | audio swap | app (retired runtime) |
| `CompiledPlan` | loader thread | audio swap | app (retired plan) |
| asset map / buffers | app or loader | audio swap | app (retired asset map) |
| DSP buffers | loader thread | audio render | app (retired runtime) |

## 8. Invariants

1. The audio callback never calls `lock`, `prepare`, plugin loading, file I/O, or
   an unbounded loop over document data.
2. The audio thread never allocates. It may move pointers and swap boxes.
3. A sync point under contention is skipped, never waited on.
4. A control update without a transport update never changes the playhead.
5. A control update without a prepared runtime never resets DSP state.
6. A pending prepared runtime survives any number of non-prepared publishes until
   the audio thread consumes it.
7. Retired audio-side allocations return through the exchange and are dropped on
   the app thread.
8. Structural values are fully prepared before they are published.
9. Offline rendering and unit tests may still use `Engine::apply`/`process`
   directly; the shared exchange path is the real-time path.
10. The final interleaved output is hard-clipped to +10 dBFS and non-finite
    samples are replaced with silence. This protects speakers from runaway plugin
    output when a plugin is enabled, disabled, or reloaded.
11. The app only records a live playhead into the document on save; it does not
    publish it back as a seek while the engine owns playback.

## 9. Input monitoring and recording capture

Recording reuses the control exchange boundary. The app opens one input stream
per selected device. Each armed track gets a preallocated slot in
`notwig-engine::InputBus`:

- the input callback selects the configured channel and pushes samples into a
  bounded monitoring queue and, while a take is active, a recording queue;
- the audio thread drains the monitoring queue into the armed track's input
  buffer before the track's device chain runs, so monitoring follows devices,
  mixer, pan, mute/solo, and routing;
- a writer thread drains recording queues into a temporary take without doing
  file I/O in either audio callback;
- routing tables are published through `EngineControlUpdate.input_routes` and
  swapped without allocating on the audio thread;
- queue overflow marks the take invalid instead of silently truncating it.

Native recording runs in the engine process. The UI process sends input
configuration, start, and stop requests over IPC, and the engine process returns
the captured buffers for finalization. Browser recording keeps the input hub and
engine in the same process and drains capture queues from the UI event loop.

Finalization writes a float WAV asset, builds its waveform, registers the asset,
clears the destination range, and creates the recorded clip in one batch.

## 10. Tests

- `notwig-engine/tests/control.rs` covers the exchange, interval gating,
  prepared-runtime installation and durability across non-prepared publishes,
  transport application, playhead publication, and metronome-only updates.
- `notwig-engine/tests/render.rs` and `tests/pdc.rs` retain offline behavior
  parity, including re-priming sustained notes after a runtime swap or seek.
- `notwig-app/tests/transport.rs` exercises transport timing through the app.
- `notwig-app/tests/workspace.rs` covers async engine preparation and the
  app-side control path.
- `notwig-engine/tests/input.rs` covers monitoring injection through the track
  chain.
- `notwig-core/tests/recording.rs` covers record arm, input selection, and
  transport recording state.
- `notwig-app/tests/recording.rs` covers recorded asset and clip finalization.
