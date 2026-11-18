# Historical planning notes

This directory preserves the original design and contract notes from the
planning phase. It is **not** the source of truth for the current code.

Use the stable documentation instead:

- [`docs/README.md`](../docs/README.md)
- [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md)
- [`docs/CODE_STRUCTURE.md`](../docs/CODE_STRUCTURE.md)
- [`docs/FEATURES.md`](../docs/FEATURES.md)

## What is still useful here

- [`AUDIO.md`](./AUDIO.md) — audio stability and import/export hardening notes.
- [`PERFORMANCE.md`](./PERFORMANCE.md) — profiling observations and follow-ups.
- [`crates/`](./crates/) — historical per-crate design contracts. Some details
  predate the current implementation (for example, undo/redo is now implemented);
  verify against the code and the stable docs before relying on them.
