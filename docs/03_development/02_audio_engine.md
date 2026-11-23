# Audio engine

Status: implemented.

The engine spans the app/UI thread, loader threads, the render worker, and the
device callback. This page covers the control exchange that connects them, the
live monitoring and recording paths, and the latency consequences of each
execution mode.

Related: [`Architecture`](00_architecture.md),
[`Code Structure`](01_code_structure.md), and the
`crates/notwig-engine/README.md`.

## Reading guide

This page has three distinct concerns. Start with the part that matches your
question instead of reading top to bottom.

| If you need to... | Read |
| --- | --- |
| Understand how UI changes reach the audio thread | Sections 1–8 |
| Understand live monitoring latency, rings, and recording alignment | Section 9 |
| Find the tests guarding this behavior | Section 10 |

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
   has not consumed yet. Do the same for a pending transport update, so a
   control publish immediately after Play cannot cancel the new transport.
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
the audio callback. In the native engine process, `EngineWorker` also keeps the
current runtime active while a dedicated loader thread builds the replacement.
Transport, control, recording, and snapshot messages continue to be handled
during the build. Superseded builds are queued and their results discarded, so
the last requested plan is the only one installed. The actual swap remains a
prepared-runtime publish and is applied at the next audio-thread sync point.

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

## 9. Monitoring latency and recording capture

This section follows one monitored sample from the input device to the speaker,
then shows how the same capture path feeds recording. It separates three things
that are easy to confuse:

1. device and driver latency, which Notwig cannot remove;
2. callback block buffering, which is the "two blocks" rule of thumb;
3. Notwig's worker and ring path, which adds latency only in specific
   configurations.

### 9.1 Vocabulary

| Term | Meaning |
| --- | --- |
| Device block `B` | Frames requested from the output device per callback. Set by **Device Block Size**. |
| High block `H` | Project render chunk. Set by **High latency buffer**. |
| Input block | Frames delivered by one input callback. It follows the device block. |
| Output block | Frames written by one output callback. It follows the device block. |
| High ring | Worker-to-callback queue for project audio and high-latency monitoring. |
| Low ring | Worker-to-callback queue for **Low Latency** track monitoring. |
| InputBus | Preallocated queues between the input callback, the engine, and the recorder. |

All block values are frames. Convert to time with:

```text
milliseconds = frames / sample_rate * 1000
```

### 9.2 Why the software path is described as two blocks

A monitored input sample crosses two independent device buffers:

```text
time ──────────────────────────────────────────────────────────────►

input device       [ capture block n ][ capture block n + 1 ]
                              │
                              │ input callback publishes block n
                              ▼
InputBus                      [ block n ]
                              │
                              │ next output callback drains block n
                              ▼
output device                  [ output block n ][ output block n + 1 ]
                                                   │
                                                   ▼
                                                 speaker
```

There are two block-sized waits:

- **Input block:** a sample is not available to software until its input callback
  has filled and published the capture block. This can be almost one full block
  for a sample captured just after the callback starts.
- **Output block:** the output callback fills a block that the device consumes
  over the next block period. The output buffer therefore contributes another
  block to the practical callback-to-speaker delay.

That is the source of the common estimate:

```text
software monitoring ≈ one input block + one output block ≈ 2B
```

It is an estimate, not a sample-exact promise. The input and output callbacks are
separate streams, so their relative phase can add part of another block. The
monitor queue is a bounded elastic buffer, not a third permanent latency stage.
Driver safety offsets and converter latency are also outside this estimate.

For a 16-frame device block:

| Sample rate | One block | Two blocks |
| --- | ---: | ---: |
| 44.1 kHz | 0.363 ms | 0.726 ms |
| 48 kHz | 0.333 ms | 0.667 ms |
| 96 kHz | 0.167 ms | 0.333 ms |

Those are buffer estimates only. Real microphone-to-speaker latency is usually a
few milliseconds because it also includes the input and output hardware paths.
CPAL and the driver may also choose a callback size other than the requested
Device Block Size; the actual callback frame count is authoritative.

### 9.3 Monitoring modes

| Track mode | Audio path | Runs where | Notes |
| --- | --- | --- | --- |
| **Disabled** | Input is discarded | Input callback / monitor drain | No monitoring signal reaches the master. |
| **Enabled** | Track devices, sends, groups, master | Project render path | With higher-block project rendering this follows the high project block. |
| **Low Latency** | A second instance of only the track device chain | Low monitor path | Bypasses groups, sends, and master. Behaves like **Enabled** when higher-block project rendering is off. |

**Enabled** monitoring is the full project path. If the project is one high
block behind, monitoring is one high block behind too. **Low Latency** avoids that
project delay, but it still has the input buffer, the monitor handoff, the output
buffer, and any latency introduced by the duplicated track devices.

### 9.4 Native execution paths

The path depends on whether the project is rendered in a larger block than the
device:

| Higher-block rendering | Device block vs high block | Engine runs in | Low Latency monitoring | Low ring |
| --- | --- | --- | --- | --- |
| Off | Equal by definition | Output callback | Falls back to **Enabled** | No |
| On | Device block smaller | `notwig-project-render` worker | Dedicated monitor chain | Yes |
| On | Device block equal | Output callback | Dedicated monitor chain | No |
| Browser | No native worker | Output callback | Dedicated chain when enabled | No |

The worker path is selected by the condition in `crates/notwig-app/src/audio.rs`:

```rust
settings.low_latency_monitoring && device_block < project_frames
```

#### Inline path

```text
input callback ──► InputBus ──► output callback ──► device
                                      │
                                      ├─ project graph and Enabled monitoring
                                      └─ Low Latency monitor chain
```

The output callback owns the engine. There are no rings and no worker handoff.
All DSP, including the low monitor chain, runs inside the device callback.

#### Worker path

```text
input callback ──► InputBus monitor queues
                              │
                              ▼
                    notwig-project-render
                    │                   │
                    │ high project      │ low monitor chain
                    │ steps             │ (duplicate track devices)
                    ▼                   ▼
                 high ring           low ring
                    │                   │
                    └────────┬──────────┘
                             ▼
                    output callback ──► device
```

The high ring is filled to the device block plus one high block. Project audio
is therefore intentionally delayed by the high block, while Low Latency tracks
bypass that queue. The worker owns the engine and plugin instances. The output
callback never calls the engine. It only pops samples from the high and low
rings, mixes them, and writes the device buffer. The high project render is split
into bounded steps (currently 64 frames, or the device block when it is larger)
so the worker can service the low ring between steps instead of disappearing for
a full high block.

### 9.5 What the low ring is and why it exists

The low ring is a **worker-fed, preallocated queue** of interleaved stereo monitor
audio. It uses a second instance of each Low Latency track's device chain,
prepared during the structural rebuild that installs the monitor mode and kept in
sync by ordinary parameter updates. In the worker path:

1. the worker drains input from `InputBus`;
2. it runs each Low Latency track's duplicate device chain at the device block
   size;
3. it applies track gain, pan, mute, and solo;
4. it writes the resulting stereo frames into the low ring.

The output callback consumes from that ring. The ring is not a processing stage;
it is a handoff buffer between two threads with different deadlines.

It is needed because the engine was moved to the worker so large project chunks
and plugin work never run in the device callback. The callback must always return
within its deadline, while the worker can be delayed by scheduling, a heavy
project step, or another plugin. The ring absorbs that jitter. Without it, the
callback would frequently run dry and produce dropouts.

The cost is the ring's target depth. A ring only prevents underruns if it
contains some audio before the callback asks for it. Today the target is:

```text
max(one high render step, sample_rate / 500)
```

At 48 kHz with a 16-frame device block that is roughly `max(64, 96)` frames, or
about 2 ms. Those queued samples are added directly to microphone-to-speaker
latency.

Illustrative software-only budget at 48 kHz with a 16-frame device block:

| Path | Input block | InputBus queue | Low ring | Output block | Software subtotal |
| --- | ---: | ---: | ---: | ---: | ---: |
| Higher-block off, inline | 0.33 ms | 0–0.33 ms | — | 0.33 ms | ~0.7–1.0 ms |
| Higher-block on, Low Latency | 0.33 ms | ~0.33 ms | ~2.0 ms | 0.33 ms | ~3.0 ms |

The table excludes driver safety offsets, converters, and any latency added by
track devices. It explains why the same 16-frame device block can feel different
in the two modes.

The low ring exists only when the worker path is active and the track is in
**Low Latency** mode. A 16-frame block with higher-block rendering off has no low
ring. With higher-block rendering on, **Enabled** tracks use the high project
ring and therefore inherit the high project block delay instead.

### 9.6 Input capture and recording alignment

Recording reuses the same capture boundary but has a different success criterion:
the captured audio must line up with the project timeline when the take is
finished, rather than be heard immediately.

The app opens one input stream per selected device and restricts a session to one
device. Named mono and stereo inputs are aliases for channels on that device. Each
armed track receives one preallocated slot in `notwig-engine::InputBus`.

```text
input callback
    │  timestamp capture
    │  select mono/stereo channels
    ▼
monitor queue ──► live monitor path (bounded backlog)
    │
record queue ───► writer thread ──► temporary take ──► WAV asset and clip
```

Key behaviors:

- **Latency measurement:** the native input callback derives capture-to-callback
  latency from CPAL's input timestamps and publishes it through `InputBus`.
  This is a measurement of the input callback path, not a delay added to live
  monitoring.
- **Recording trim:** new takes are trimmed by the measured input plus output
  latency, with the device block as a floor and a one-second cap.
- **Backlog control:** monitor queues drop stale samples beyond two requested
  blocks or 5 ms. This bounds latency if clocks drift or one path stalls; dropped
  monitor audio is preferable to permanently lagging monitoring.
- **System Out:** input resolution prefers the default output device when it also
  exposes inputs, keeping capture and playback on one clock domain. If the output
  has no inputs, recording falls back to the default input device and logs the
  separate-clock risk.
- **Recording writer:** a writer thread drains the record queues into a
  temporary take. Neither audio callback performs file I/O.
- **Overflow:** record queue overflow marks the take invalid instead of silently
  truncating it.
- **Finalization:** the take is written as a float WAV asset, its waveform is
  built, the destination range is cleared, and the recorded clip is created in
  one batch.

Native recording lives in the engine process; the UI process sends input
configuration and start/stop requests over IPC. While recording, captured chunks
are forwarded to the UI process so the live clip and waveform can grow. Browser
recording keeps capture and rendering in one process and drains queues from the
UI event loop. The browser has no CPAL input backend: `audio_input::web` uses
`getUserMedia` and a `ScriptProcessorNode` to push the same interleaved samples
into `InputBus`.

### 9.7 Invariants

1. In the worker path, the output callback never touches the engine, plugins, or
   document state.
2. Ring buffers are preallocated. Production and consumption are bounded and
   never wait for each other.
3. The low ring is a handoff buffer, not a processing stage. Increasing its
   target lowers underrun risk but increases monitoring latency.
4. Low Latency monitoring bypasses groups, sends, and the master chain; it is
   not a full-project monitor.
5. When higher-block project rendering is off, Low Latency behaves like Enabled
   and uses neither the worker nor the low ring.
6. Monitor backlog is bounded so a stalled path cannot accumulate permanent
   latency.
7. Recording alignment uses measured device timestamps; live monitoring uses the
   real signal path and is not artificially delayed by that measurement.

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
