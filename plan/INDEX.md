# Notwig Planning Index

Precise architecture/contracts for v0 (planning phase). Source intent: `README.md` and clarified scope decisions.

## High-Level (`high-level/`)
- [`high-level/INDEX.md`](./high-level/INDEX.md) — common entrypoint.
- [`high-level/OVERVIEW.md`](./high-level/OVERVIEW.md) — merged architecture and cross-cutting contracts.
- [`high-level/INTERACTIONS.md`](./high-level/INTERACTIONS.md) — system interactions/data flow.

## Crate Plans (`crates/`)
- [`crates/notwig-core/INDEX.md`](./crates/notwig-core/INDEX.md) — domain model, tracks, sequencer, protocol (`Command/Delta/EngineMsg/RtEvent`).
- [`crates/notwig-project/INDEX.md`](./crates/notwig-project/INDEX.md) — lifecycle, bundle persistence, migration, autosave/recovery, assets.
- [`crates/notwig-dsp/INDEX.md`](./crates/notwig-dsp/INDEX.md) — devices, parameters, graph/plan, compilation, RT/offline, stock devices, testing.
- [`crates/notwig-engine/INDEX.md`](./crates/notwig-engine/INDEX.md) — engine execution and realtime/offline rendering.
- [`crates/notwig-plugins/INDEX.md`](./crates/notwig-plugins/INDEX.md) — plugin hosting (v0 in-process: VST3/AU primary, VST2 fallback).
- [`crates/notwig-ui/INDEX.md`](./crates/notwig-ui/INDEX.md) — UI architecture, primitives, views, interaction, device UI.
- [`crates/notwig-app/INDEX.md`](./crates/notwig-app/INDEX.md) — app composition, runtime/thread topology, wiring.

## Code (`code/`)
- [`code/INDEX.md`](./code/INDEX.md) — workspace-level code planning index.
- [`code/WORKSPACE.md`](./code/WORKSPACE.md) — Cargo workspace, members, dependency/feature/profile contracts.
- [`code/CRATES.md`](./code/CRATES.md) — per-crate responsibilities and boundaries.
- [`code/CONVENTIONS.md`](./code/CONVENTIONS.md) — conventions.

## v0 Focus
- Fundamentals: Project bundle, Tracks, Arranger, MIDI/Audio clips (basic playback/edit; notes-only MIDI), hosted instruments, Device system (EQ/Tool/Chain), RT-safe engine boundaries, polished reusable UI primitives/workflows.
- Platform: macOS first (VST3 + AU; VST2 fallback); portable architecture for later Windows/Linux.
- Explicitly deferred: Clip Launcher, recording, live input/monitoring, stock instruments, metronome/count-in, warping, tempo automation, modulation/macros runtime, undo/redo, comping, collaboration, out-of-process plugins.
