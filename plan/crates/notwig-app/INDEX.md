# `notwig-app` Planning

Composition root and platform/bootstrap crate.

## Contracts
- [`APP.md`](./APP.md) — process wiring, runtime/thread topology, channel plumbing, startup/shutdown, integration of all crates.

## Boundaries
- Depends on other workspace crates as needed.
- Contains no reusable domain/DSP logic that belongs in lower crates.
