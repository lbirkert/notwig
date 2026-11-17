# High-Level Index

Common entrypoint for understanding Notwig's planned systems.

## Files
- [`OVERVIEW.md`](./OVERVIEW.md) — merged architecture and cross-cutting contracts (scope, decisions, invariants, topology, time model, interfaces, validation, extensibility).
- [`INTERACTIONS.md`](./INTERACTIONS.md) — system map and interaction/data-flow overview between Core, Project, DSP/Engine, Plugins, UI, and App.

## Crate-aligned planning
Detailed contracts are organized by crate under `../crates/` (keep `../code/` for workspace-level code planning):
- [`../crates/notwig-core/`](../crates/notwig-core/INDEX.md), [`../crates/notwig-project/`](../crates/notwig-project/INDEX.md), [`../crates/notwig-dsp/`](../crates/notwig-dsp/INDEX.md), [`../crates/notwig-engine/`](../crates/notwig-engine/INDEX.md), [`../crates/notwig-plugins/`](../crates/notwig-plugins/INDEX.md), [`../crates/notwig-ui/`](../crates/notwig-ui/INDEX.md), [`../crates/notwig-app/`](../crates/notwig-app/INDEX.md)

## Inspiration & Dependencies
Use these as implementation references or candidate dependencies. Notwig remains clean-room with respect to Bitwig/OpenLive: no code/assets copied from them. Any external dependency requires license/API review before adoption.

- [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — VST3, AU, and CLAP plugin loading; candidate dependency for Notwig's `HostAdapter` and the macOS-first VST3/AU hosting path. CLAP is not a v0 target format.
- [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — existing Bitwig-inspired clone. Take UI/workflow inspiration only, while keeping v0 to a smaller, higher-quality feature subset. Do not copy code or assets.
