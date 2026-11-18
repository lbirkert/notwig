# notwig-engine

Real-time playback and offline rendering.

## Purpose

`notwig-engine` executes a compiled plan. It owns the authoritative runtime
playhead, transport state, track/device runtime instances, metering, and RT
events. The document is never mutated here.

## Owns

- `Engine`: plan/schedule application, transport, block processing, and offline
  rendering.
- `EngineConfig`: sample rate, channel count, and maximum block size.
- `ActivePlan` and track/device runtime state.
- Block chunking when a host block exceeds `max_block_size`.
- All-notes-off and tail rendering on stop.
- Meter and status events delivered through `RtEvent`.
- Device factories for stock and hosted plugins.

## Does not own

- `ProjectDoc` mutation or project persistence.
- The OS audio stream (see `notwig-app/src/audio.rs`).
- Device specifications or graph compilation (see `notwig-dsp`).
- Plugin scanning or loading policy (see `notwig-plugins`).
- UI state or meters rendering.

## Real-time rules

The audio callback path should not allocate, block, perform file/plugin I/O, or
run unbounded work. The engine queues messages and events; heavy plan builds
happen off-thread in `notwig-app`.

## Module map

| Module | Contents |
|---|---|
| `engine.rs` | `Engine`, transport, process/offline render |
| `runtime.rs` | `ActivePlan`, `TrackRuntime`, event collection |
| `factory.rs` | `DeviceFactory`, stock and plugin factories |

## Tests

`tests/render.rs` covers block chunking, transport, stop note-offs, and offline
rendering. `notwig-app` tests cover the async build/swap path end to end.
