# `notwig-engine` Planning

Realtime/offline audio engine execution crate.

## Contracts
- [`ENGINE.md`](./ENGINE.md) — responsibilities, lifecycle, control/RT boundaries, scheduling, integration with Core/DSP/Plugins.
- [`RENDERING.md`](./RENDERING.md) — RT/offline execution model, block processing, transport/playhead/meter emission, configuration changes.

## Boundaries
- Depends on `notwig-core`, `notwig-dsp`, `notwig-plugins`.
- Owns `S_rt`/active plan execution; never mutates `S_doc`.
- RT callback must satisfy `notwig-dsp/REALTIME_OFFLINE.md`.
