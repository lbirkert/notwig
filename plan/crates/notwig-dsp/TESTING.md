# DSP Testing & Verification (`notwig-dsp`)

## 1. Correctness
- Unit tests per primitive/device (gain/pan/filter/mix/fades).
- Reference/signal tests with tolerances `ε` for known vectors.
- EQ/filter response assertions on `H(f)` and time-domain properties where applicable.
- Numerical stability: extremes, long runs, denormals, NaN/Inf guards.
- State round-trip equivalence for devices/chains.
- Scalar vs SIMD parity within `ε_simd`.

## 2. Integration & Semantics
- Graph compilation, buffer routing, and PDC correctness (including chains/containers/nesting).
- Realtime/offline render equivalence within `ε(cfg)`.
- v0: verify reserved modulation/automation do not affect processing.

## 3. RT Safety & Performance
- Stress tests at target `sr/N`; xrun and bounded-queue behavior.
- Instrumented checks for no alloc/lock in RT path (debug/CI where feasible).
- Benchmarks: CPU/block, offline throughput, memory; no perf claims without data + correctness.

## 4. Acceptance
- Deterministic for fixed inputs/configs; regressions gated by thresholds.

## 5. Provisional Targets (v0)
- Sample rates: 44.1/48/88.2/96 kHz; block sizes 64–1024 frames; reference config 48 kHz / 256 frames.
- Stock-DSP f32 time-domain tolerance: `1e-6` absolute per sample nominal; EQ/filter magnitude within 0.1 dB where specified. Exact `ε` values finalized from measured data.
- Reference RT workload: 32 stereo tracks with representative stock devices and a few hosted plugins, xrun-free on baseline macOS hardware. No performance claims without benchmarks.
- Block-size invariance and realtime/offline equivalence are mandatory tests for stock DSP; hosted-plugin results are reported, not gated bit-exactly.
