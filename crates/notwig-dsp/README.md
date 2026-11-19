# notwig-dsp

Device contracts, parameter definitions, and playback-plan compilation.

## Purpose

`notwig-dsp` describes what a device is, how its parameters behave, and how a
`ProjectDoc` becomes a renderable plan. It is the boundary between the editable
document and the real-time engine. It does not run the audio callback and does
not persist device instance state.

## Owns

- `DeviceSpec`, ports, signal types, processing context, and note events.
- `DeviceProcessor` and related real-time device traits.
- Parameter specifications, ranges, smoothing, and normalized mapping.
- `DeviceRegistry` for resolving type IDs to specs and processors.
- Graph compilation from `ProjectDoc` into a `CompiledPlan` with track/device
  order, sends, chains, and per-track input/output latency labels for
  compensation.
- Stock device definitions and processors: EQ, Tool, and Chain.
- EQ magnitude response used by the UI.

## Does not own

- The audio callback or transport (see `notwig-engine`).
- Persisted `DeviceInstance` state (see `notwig-core`).
- Plugin discovery or hosting (see `notwig-plugins`).
- Device UI layout (see `notwig-ui`).

## Module map

| Module | Contents |
|---|---|
| `devices.rs` | Device/processor traits, ports, process context |
| `params.rs` | Parameter specs, ranges, smoothing |
| `registry.rs` | Type ID resolution |
| `graph.rs` | `compile` and `CompiledPlan` |
| `stock.rs` | EQ/Tool/Chain specs and processors |
| `smoothing.rs` | Smoothing primitives |

## Tests

`tests/stock.rs` covers stock device behavior and plan compilation.
`tests/graph.rs` covers bypass, nested-chain, and inactive-track latency.
DSP changes should include a rendering/behavior test, not only a compile check.
