# Realtime & Offline Contracts (`notwig-dsp`)

## 1. RT Safety Invariants
In audio callback and all RT-path callees:
- No heap alloc/free, no blocking locks/waits, no filesystem/IPC/network, no logging that allocates/blocks.
- No UI/plugin GUI calls, no scan/load/compile.
- Bounded execution per block; bounded queues only.

Allowed: preallocated buffers, lock-free bounded queues, atomics, precomputed plans.

## 2. Numeric & Audio Environment
- Primary sample format `f32` (internal `f64` allowed where specified).
- Config: `sampleRate`, variable `blockSize (N)`, channel layout; reprepare on changes (non-RT) and apply safely.
- Mono/stereo baseline, multichannel-capable contracts.
- Denormal mitigation and smoothing to avoid spikes/clicks.

## 3. Offline Rendering
- Uses same semantic graph/scheduling/contracts as RT for given `S_doc` and config.
- May use larger blocks and non-RT allocations/parallelism outside semantic kernel equivalence.
- Target equivalence: `|y_off - y_rt| <= ε(cfg, dtype, devices)` for identical inputs/schedule.
- Semantic output is block-size invariant (except explicitly documented stateful analysis/metering); tests MUST verify this or state an explicit tolerance.
- Implicit sample-rate conversion is not part of the engine: audio assets are already at the project rate, and changing the project rate is unsupported in v0.
- Determinism applies to stock DSP and deterministic plugins. Hosted plugins may be non-deterministic or block-size sensitive; they are excluded from bit-exact guarantees and validated within reported/measured tolerance where feasible.

## 3.1 Overload & Error Fallback (provisional v0)
- On overload/device error, the engine performs a short click-free fade to silence, continues processing later blocks, and reports `RtEvent::Status/Error`; it never repeats stale buffers.

## 4. Backend Abstraction
- Semantics separated from realization (`ScalarCPU` baseline in v0; other CPU/SIMD variants optional). Non-CPU backends deferred and must not alter v0 RT semantics.
