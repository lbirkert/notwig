# notwig-core

The canonical domain model and protocol for Notwig.

## Purpose

`notwig-core` defines the project document and the pure operations that change
it. It is the lowest layer of the workspace and has no workspace dependencies.
Everything that needs to describe project state, time, tracks, clips, devices,
or commands uses the types in this crate.

## Owns

- `ProjectDoc` and its validation.
- Typed IDs (`Id<T>`, `TrackId`, `ClipId`, `DeviceId`, ...).
- Musical and audio time, ranges, units, colors, and `TempoMap`.
- Track kinds, mixer state, routing, and track forest projections.
- Arranger clips: MIDI notes and audio slices.
- Command/engine protocol types: `CommandPayload`, `EngineMsg`, `RtEvent`,
  `PlaybackSchedule`, and `PlanHandle`.
- `apply(doc, command)`, which returns the new document, a delta, and engine
  messages.
- The shared editing algebra in `edit/`.

## Does not own

- File or bundle I/O (see `notwig-project`).
- UI or `egui` state (see `notwig-ui`).
- Real-time audio execution (see `notwig-engine`).
- Plugin loading (see `notwig-plugins`).
- ID allocation or randomness. Callers supply stable IDs for created entities.

## Features

- `rand` (default): enables UUID v4 generation for command-application paths
  that still allocate IDs internally. Disable it for wasm extension builds that
  only need domain types; those paths fall back to a deterministic counter.

## Module map

| Module | Contents |
|---|---|
| `domain.rs` | `ProjectDoc`, metadata, assets, devices, chains, transport |
| `ids.rs` | Typed opaque entity IDs |
| `time.rs` | `TimeMus`, `TimeAudio`, `RangeMus`, `Rational`, `TempoMap`, units |
| `tracks.rs` | Track kinds, mixer, routing, `TrackForest` |
| `sequencer.rs` | `ClipEntity`, `MidiClipData`, `AudioClipData`, `AudioSlice` |
| `protocol.rs` | Commands, engine messages, RT events, schedules |
| `apply.rs` | Command application and validation |
| `edit/` | Editor-agnostic edit algebra and arranger adapter |

## Public surface

The crate re-exports its stable domain types from `lib.rs`. Internal helpers
stay private. New public items should be coherent, documented, and intended as
part of the crate contract.

## Tests

Integration tests live in `tests/`:

- `edit_algebra.rs` — algebra unit tests, laws, and property tests.
- `domain_flow.rs` — command application and document flows.
- `time.rs`, `ids.rs` — time and identity behavior.

See [`docs/EDITING.md`](../../docs/EDITING.md) for the editing contract.
