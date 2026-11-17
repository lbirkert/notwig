# AGENTS.md

## Project gist
Notwig is an open-source **clean-room reimplementation** of a Bitwig-inspired DAW, currently in the **planning/specification phase**. The goal is to define precise contracts for implementation (by AI agents/humans), not to copy code or assets from Bitwig or other projects.

- **Priorities:** extensibility, simplicity, few high-quality features, stable/correct real-time DSP.
- **Tech direction:** Rust (Cargo workspace), UI with `egui` + `wgpu`.
- **v0 focus:** project bundle persistence, tracks, Arranger-only sequencer, basic MIDI/audio clip editing (playback/edit only), hosted instruments, device system (EQ/Tool/Chain), RT-safe engine boundaries, polished reusable UI primitives/workflows.
- **Deferred in v0:** Clip Launcher/scenes, audio/MIDI recording, live input/monitoring, stock instruments, metronome/count-in, audio warping, tempo/time-signature automation, modulation/macros runtime, undo/redo, comping/takes, collaboration, out-of-process plugin hosting, Windows/Linux builds.

## Where to find planning docs
Start here:
- [`plan/high-level/INDEX.md`](plan/high-level/INDEX.md) — common entrypoint: system map and interactions.
- [`plan/high-level/OVERVIEW.md`](plan/high-level/OVERVIEW.md) — merged architecture and cross-cutting contracts (scope, decisions, invariants, topology, interfaces).
- [`plan/INDEX.md`](plan/INDEX.md) — master index of all planning/contract documents.

Crate-aligned contracts (primary detail):
- [`plan/crates/notwig-core/INDEX.md`](plan/crates/notwig-core/INDEX.md) — domain, tracks, sequencer, protocol (`Command/Delta/EngineMsg/RtEvent`).
- [`plan/crates/notwig-project/INDEX.md`](plan/crates/notwig-project/INDEX.md) — project lifecycle and bundle persistence.
- [`plan/crates/notwig-dsp/INDEX.md`](plan/crates/notwig-dsp/INDEX.md) — devices, parameters, graph/plan, compilation, RT/offline, stock devices.
- [`plan/crates/notwig-engine/INDEX.md`](plan/crates/notwig-engine/INDEX.md) — engine execution and rendering.
- [`plan/crates/notwig-plugins/INDEX.md`](plan/crates/notwig-plugins/INDEX.md) — plugin hosting (v0 VST3/AU primary, VST2 fallback).
- [`plan/crates/notwig-ui/INDEX.md`](plan/crates/notwig-ui/INDEX.md) — UI architecture, primitives, views, interaction.
- [`plan/crates/notwig-app/INDEX.md`](plan/crates/notwig-app/INDEX.md) — composition and runtime wiring.

Workspace-level code planning:
- [`plan/code/INDEX.md`](plan/code/INDEX.md) — Rust workspace/crate boundaries and conventions.

Other references:
- [`README.md`](README.md) — project summary, paradigms, and inspiration links.
- Inspiration / candidate dependencies (license review required; clean-room w.r.t. Bitwig/OpenLive): [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) (VST3/AU/CLAP loading; may be used as the plugin-hosting dependency), [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) (Bitwig-inspired UI/workflow; ideas only).
