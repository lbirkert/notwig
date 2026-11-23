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
- Device contracts, parameter definitions, plan compilation, stock devices,
  factories, and editor monitoring in `dsp/`.

## Does not own

- File or bundle I/O (see `notwig-project`).
- UI or `egui` state (see `notwig-ui`).
- Real-time audio execution (see `notwig-engine`).
- Plugin loading (see `notwig-plugins`).
- Device UI layout (see `notwig-ui`).
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
| `protocol/` | Commands, deltas, queries, plans, schedules, engine messages, RT events |
| `apply/` | Command application by family: transport, tracks, clips, notes, devices, assets, audio; `apply` enforces an end-state document validation |
| `edit/` | Edit algebra (`algebra/`: materialize, delete, paste, move, trim) and arranger adapter |
| `dsp/devices.rs` | Device/processor traits, ports, process context |
| `dsp/params.rs` | Parameter specs, ranges, smoothing, normalized mapping |
| `dsp/registry.rs` | Type ID and highest-version resolution |
| `dsp/graph.rs` | `compile` and `CompiledPlan` |
| `dsp/stock/` | Stock specs, Tool, EQ, Chain, and shared biquad math |
| `dsp/factory.rs` | `DeviceFactory`, stock factory, and composite factory |
| `dsp/monitor.rs` | Lock-free audio monitor ring for plugin editor instances |
| `dsp/smoothing.rs` | Smoothing primitives |

## Public surface

The crate re-exports its stable domain types from `lib.rs`. Internal helpers
stay private. New public items should be coherent, documented, and intended as
part of the crate contract.

## Tests

Integration tests live in `tests/`:

- `edit_algebra.rs` — algebra unit tests, laws, and property tests.
- `domain_flow.rs` — command application and document flows.
- `apply_validation.rs` — end-state validation at the `apply` boundary.
- `tempo_audio.rs` — sample-locked audio rescaling and lane overlap resolution.
- `midi_notes.rs` — MIDI note command validation and editing.
- `schedule_emission.rs` — playback schedule extraction from the document.
- `dsp_stock.rs` — stock device behavior, including allocation-free chain blocks.
- `dsp_graph.rs` — bypass, nested-chain, latency, and registry version selection.
- `dsp_container.rs` — host-owned container routing, wet path, and nesting.
- `time.rs`, `ids.rs` — time and identity behavior.

## Benchmarks

`benches/dsp.rs` uses Criterion to measure Tool, EQ, Chain, plan compilation, EQ
magnitude response, and pan gains:

```sh
cargo bench -p notwig-core
```

See [`docs/03_development/03_editing.md`](../../docs/03_development/03_editing.md) for the editing contract.
