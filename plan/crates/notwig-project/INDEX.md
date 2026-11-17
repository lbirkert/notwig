# `notwig-project` Planning

Project lifecycle and persistence boundary crate.

## Contracts
- [`PROJECT.md`](./PROJECT.md) — operations, canonical `*.notwig/` bundle, schema versioning/migration, autosave/recovery, asset index and relink.

## Boundaries
- Depends on `notwig-core` (document/types/protocol-relevant structures).
- Performs file IO and asset management off RT thread; no UI logic, no RT processing.
