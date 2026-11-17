# Rust Code & API Conventions (Planning Contract)

## 1. Module & API Design
- One responsibility per crate/module; public surface minimal (`pub(crate)` by default internally).
- Prefer explicit types/newtypes for IDs, time domains, units, and signal kinds: `Id<T>`, `TimeMus`, `TimeAudio`, `Db`, `Norm01`, etc.
- Deterministic behavior: stable iteration/order where semantically relevant (e.g. track/chain/param order) using ordered collections.
- Pure functions for projections/validation where possible: `π : (S_doc,S_rt,S_ui)->V`, `validate/apply` without hidden global state.
- ID allocation: entity-creating commands carry caller-generated IDs (UUIDv4). Lower-level domain code never allocates IDs or uses randomness; tests use fixed IDs for determinism.
- Persisted data and generated artifacts use canonical encodings (sorted keys, stable hashes) so save/load and caches are reproducible.

## 2. Error Handling
- Fallible operations return `Result<T,E>` with typed error enums per boundary (`CoreError`, `ProjectError`, `EngineError`, `PluginError`, ...).
- No panics for recoverable external/input failures (files, plugins, commands); panics only for internal invariant violations (debug-asserted where appropriate).
- RT path: no unwinding across audio callback boundary; errors reported via `RtEvent`/status.

## 3. Serialization & Schema
- Persisted and message types versioned where required (`SchemaVersion`, type/version tags).
- Migrations explicit and testable; unknown incompatible major versions rejected.
- Stable field/ID semantics prioritized over incidental representation.

## 4. Concurrency & RT Safety
- Ownership model enforces separation: `S_doc` mutated only in Core context; engine owns RT state/plan.
- Cross-thread communication via bounded channels/lock-free queues; shared immutable data `Arc`/snapshots where safe.
- RT-annotated modules/functions MUST NOT: allocate, lock/block, perform IO/syscalls, log synchronously, or call UI/plugin GUI/scan/load/compile.

## 5. Testing & Verification
- Unit tests colocated per crate for domain logic, serialization/migrations, graph/param/device behavior.
- Integration tests for command->state->engine message flows, project bundle round-trips, and realtime/offline equivalence per `dsp/TESTING.md`.
- Benchmarks gated for performance claims; CI-oriented checks for formatting/lints/tests (`cargo fmt`, `cargo clippy`, `cargo test`) as baseline (exact policy configurable).

## 6. Documentation
- Public contracts documented with invariants, units, and threading/RT constraints.
- Planning docs in `plan/` remain source of architectural truth during planning phase; code/API evolves to match.
