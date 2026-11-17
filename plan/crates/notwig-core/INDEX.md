# `notwig-core` Planning

Canonical domain/session model crate. Owns document truth (`S_doc`), validation/mutation, projections, and cross-boundary protocol contracts (v0).

## Contracts
- [`DOMAIN.md`](./DOMAIN.md) — primitives, IDs, `ProjectDoc` root, time/units, validation and `apply` semantics.
- [`TRACKS.md`](./TRACKS.md) — track kinds, track entity, hierarchy/order, routing model, mixer projection fields.
- [`SEQUENCER.md`](./SEQUENCER.md) — Arranger-only model, clips/lanes, MIDI and audio payloads, edit and scheduling contracts.
- [`PROTOCOL.md`](./PROTOCOL.md) — canonical `Command`, `StateDelta`/query results, `EngineMsg`, and `RtEvent` schemas and delivery rules.

## Boundaries
- Depends on no other workspace crates.
- No IO, UI, audio callback processing, or plugin hosting.
- Source planning: split/refined during the crate-aligned restructuring; cross-cutting context in `../../high-level/OVERVIEW.md`.
