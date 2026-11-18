# notwig

Notwig is an open-source, lightweight digital audio workstation for macOS. It
focuses on a deliberately small set of well-designed, extensible features
instead of feature parity with a large commercial DAW.

Bitwig Studio is the main reference for interaction and workflow ideas. Notwig
is not a Bitwig clone, a drop-in replacement, or an attempt to reproduce its
feature set. It is an independent project with its own smaller scope.

## Disclaimer

This project is not designed to be a replacement for Bitwig Studio. I love
Bitwig, which is exactly why I use it as a reference for interaction and
workflow ideas while building my own implementation. I want an open-source,
lightweight DAW that is easy to understand, pleasant to use, and can be
extended without carrying the weight of a full commercial feature set.

Notwig is not affiliated with or endorsed by Bitwig GmbH. No Bitwig code,
assets, or other copyrighted material is used in this project.

## What Notwig is

- An Arranger-first DAW with a linear timeline and non-destructive clip editing.
- A small, focused tool: fewer features, but each one designed and tested with care.
- An extensible codebase built as a Rust workspace with clear crate boundaries.
- A real-time audio engine that prioritizes correctness and predictable behavior.
- A macOS application today, with platform-specific code kept behind adapters.

## Scope

Notwig is an early-stage, playback/edit-only macOS prototype. Current features,
candidate next milestones, and deferred scope live in
[`docs/ROADMAP.md`](docs/ROADMAP.md).

## Build and run

Requirements: a recent Rust toolchain, macOS, and the platform audio/plugin SDKs
needed by `cpal` and `truce-rack`.

```sh
cargo run -p notwig-app
cargo run -p notwig-app -- ui path/to/project.notwig
```

CLI commands:

```sh
cargo run -p notwig-app -- new path/to/project.notwig
cargo run -p notwig-app -- demo
cargo run -p notwig-app -- render path/to/project.notwig out.wav [--start BEATS] [--end BEATS]
cargo run -p notwig-app -- info path/to/project.notwig
```

Checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index and reading paths.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system layers, state flow, and invariants.
- [`docs/CODE_STRUCTURE.md`](docs/CODE_STRUCTURE.md) — repository and module map, extension recipes.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — current features, next milestones, and deferred scope.
- [`docs/EDITING.md`](docs/EDITING.md) — editing algebra and adapter contracts.
- [`docs/EDITING_TESTING.md`](docs/EDITING_TESTING.md) — testing strategy for edit operations.

Each crate has its own `README.md` describing its exact responsibility and
boundaries.

## Inspiration

- [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — VST3/AU plugin loading.
- [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — Bitwig-inspired UI/workflow ideas.

## License

Notwig is licensed under the [Mozilla Public License](LICENSE).
