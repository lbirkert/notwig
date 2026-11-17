# `notwig-dsp` Planning

Shared DSP/device contracts and non-RT DSP utilities crate.

## Contracts
- [`DEVICES.md`](./DEVICES.md) — device kinds/entities, ports/signals, chains, processing interface contracts.
- [`PARAMETERS.md`](./PARAMETERS.md) — parameter specs, mapping, updates/smoothing, reserved automation/modulation/macro contracts.
- [`GRAPH.md`](./GRAPH.md) — processing graph model, compilation to `Plan`, buffers/events, PDC, swap contracts.
- [`COMPILATION.md`](./COMPILATION.md) — Faust/libfaust stock DSP workflow, artifacts/cache/invalidation, SIMD policy.
- [`REALTIME_OFFLINE.md`](./REALTIME_OFFLINE.md) — RT safety invariants, numeric environment, offline equivalence, backend abstraction.
- [`STOCK_DEVICES.md`](./STOCK_DEVICES.md) — foundational stock devices: `notwig.tool`, `notwig.eq`, `notwig.chain`.
- [`TESTING.md`](./TESTING.md) — correctness, integration, RT-safety/performance verification contracts.

## Boundaries
- Depends on `notwig-core` for shared IDs/types where required.
- Defines RT-safe traits/interfaces and plan/data structures; does not run the audio callback.
