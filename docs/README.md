# Notwig documentation

These documents describe the project as it exists today. They are the stable
entry point for contributors and agents; the historical planning notes under
`plan/` are not authoritative.

## Start here

| If you want to... | Read |
|---|---|
| Understand what Notwig does | [`FEATURES.md`](FEATURES.md) |
| Understand how the system is put together | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Find where code lives and how to extend it | [`CODE_STRUCTURE.md`](CODE_STRUCTURE.md) |
| Change editing behavior | [`EDITING.md`](EDITING.md) |
| Add or change tests for editing | [`EDITING_TESTING.md`](EDITING_TESTING.md) |

Each crate also has a `README.md` with its exact responsibility, public
surface, and boundaries:

- [`crates/notwig-core/README.md`](../crates/notwig-core/README.md)
- [`crates/notwig-project/README.md`](../crates/notwig-project/README.md)
- [`crates/notwig-dsp/README.md`](../crates/notwig-dsp/README.md)
- [`crates/notwig-engine/README.md`](../crates/notwig-engine/README.md)
- [`crates/notwig-plugins/README.md`](../crates/notwig-plugins/README.md)
- [`crates/notwig-ui/README.md`](../crates/notwig-ui/README.md)
- [`crates/notwig-app/README.md`](../crates/notwig-app/README.md)

## Suggested reading paths

### New to the project

1. [`FEATURES.md`](FEATURES.md) to see the current scope.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md) for the state and data-flow model.
3. [`CODE_STRUCTURE.md`](CODE_STRUCTURE.md) for the crate and module map.
4. The README of the crate you are about to change.

### Adding a user-facing feature

1. Decide which layer owns the behavior in [`CODE_STRUCTURE.md`](CODE_STRUCTURE.md).
2. Add or extend the domain command in `notwig-core` first.
3. Lower it from the UI through `notwig-app`.
4. Add the UI affordance in `notwig-ui`.
5. Test at the lowest useful layer and add an integration test at the seam.

### Changing editing behavior

Read [`EDITING.md`](EDITING.md) before touching the arranger or clip editor.
The algebra is shared; editor-specific behavior belongs in an adapter or in
lowering, not in a second copy of copy/paste/duplicate logic.

## Keeping docs current

Update the relevant document when behavior, boundaries, or public APIs change.
Prefer small, direct edits over adding a parallel explanation. If a document no
longer matches the code, fix or remove it instead of leaving a misleading note.
