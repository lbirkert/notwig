# `notwig-ui` Planning

UI implementation crate (`egui` + `wgpu`), projection/command client.

## Contracts
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — layer boundaries, `S_doc/S_rt/S_ui`, data flow, rendering/performance.
- [`PRIMITIVES.md`](./PRIMITIVES.md) — reusable design-system primitives and parameter binding contracts.
- [`VIEWS.md`](./VIEWS.md) — v0 shell and views (Arranger, Mixer, Device/Inspector, Browser, editors).
- [`INTERACTION.md`](./INTERACTION.md) — input routing, selection/focus, drag-drop, gestures, feedback, shortcuts.
- [`DEVICE_UI.md`](./DEVICE_UI.md) — device presentation and stock/generic plugin UI contracts.

## Boundaries
- Depends on `notwig-core`, `notwig-project`, `notwig-dsp` (contracts/metadata). No engine RT internals.
- Does not own canonical state; heavy work async/off frame.
