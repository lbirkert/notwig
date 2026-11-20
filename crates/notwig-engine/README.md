# notwig-engine

Real-time playback and offline rendering.

## Purpose

`notwig-engine` executes a compiled plan. It owns the authoritative runtime
playhead, transport state, track/device runtime instances, metering, and RT
events. The document is never mutated here.

## Owns

- `Engine`: plan/schedule application, transport, block processing, and offline
  rendering.
- `EngineControl`/`ControlHandle`: the app/audio control exchange.
- `EngineData`: app-owned control snapshot, optional transport update, and an
  optional prepared runtime.
- `PreparedRuntime`: a fully built `ActivePlan` ready for the audio thread.
- `RuntimeLoader`: the plugin/runtime loader thread.
- `EngineConfig`: sample rate, channel count, and maximum block size.
- `ActivePlan` and track/device runtime state.
- Block chunking when a host block exceeds `max_block_size`.
- All-notes-off and tail rendering on stop.
- Meter and status events delivered through `RtEvent`.
- Latency compensation for track routing, sends, nested chains, and bypassed
  devices, using preallocated delay lines.
- Device factories for stock and hosted plugins.

## Does not own

- `ProjectDoc` mutation or project persistence.
- The OS audio stream (see `notwig-app/src/audio.rs`).
- Device specifications or graph compilation (see `notwig-dsp`).
- Plugin scanning or loading policy (see `notwig-plugins`).
- UI state or meters rendering.

## Real-time rules

The audio callback path should not allocate, block, perform file/plugin I/O, or
run unbounded work. The app publishes control data into an exchange slot. The
audio thread tries the slot once per fixed frame interval, swaps boxes, and
applies the control data; retired values return to the app thread for disposal.
Heavy plan builds, plugin instantiation, and buffer allocation happen on the
engine build/plugin-loader thread, never on the audio thread. See
[`docs/03_development/02_audio_engine.md`](../../docs/03_development/02_audio_engine.md).

## Module map

| Module | Contents |
|---|---|
| `engine.rs` | `Engine`, transport, control sync, process/offline render |
| `control.rs` | `EngineData`, `EngineControl`, `ControlHandle`, `PreparedRuntime`, `RuntimeLoader` |
| `runtime.rs` | `ActivePlan`, `TrackRuntime`, event collection |
| `factory.rs` | `DeviceFactory`, stock and plugin factories |

## Tests

`tests/render.rs` covers block chunking, transport, stop note-offs, and offline
rendering. `tests/control.rs` covers the shared control exchange, sync interval,
prepared-runtime installation, and playhead publication. `tests/pdc.rs` covers
flat and nested latency alignment and send phase coherence. `notwig-app` tests
cover the async build/swap path end to end.
