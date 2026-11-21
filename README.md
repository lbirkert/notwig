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
candidate next milestones, and deferred scope live in the
[roadmap](docs/03_development/06_roadmap.md).

## Build and run

Requirements: a recent Rust toolchain, macOS, and the platform audio/plugin SDKs
needed by `cpal` and the vendored `notwig-rack` host. LV2 support also needs the
system `lilv` library (`brew install lilv`).

```sh
cargo run -p notwig-app
cargo run -p notwig-app -- ui path/to/project.notwig
```

Browser build:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm-pack build crates/notwig-web --target web --release \
  --out-dir ../../target/web/pkg --out-name notwig_web
mkdir -p target/web
cp web/index.html web/styles.css target/web/
python3 -m http.server --directory target/web 8080
```

The browser build stores projects in a `Projects` folder inside the Origin
Private File System (OPFS) and uses WebAudio through CPAL. Project open and save
use an in-app project picker, and **FILE > Download Project** exports the
project folder as a `.notwig.zip`. Local audio and extension ZIP files are read
through the browser's standard file input. Native VST3/VST2/CLAP/LV2/AU plugins
are unavailable. Browser extensions run through the browser WebAssembly host:
only `wasm` entrypoints are eligible, the stock extension is fetched from the
same origin, and ZIP packages can be installed into browser storage. Audio starts
after the first user gesture.

CI publishes the docs at `https://notwig.lbirkert.com/` and the app at
`https://notwig.lbirkert.com/app/`.

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

The documentation is an mdBook under `docs/`, organized by audience:

- [`docs/00_introduction.md`](docs/00_introduction.md) — what Notwig is and how
  to choose a reading path.
- [`docs/01_users/`](docs/01_users/) — user guide: workspace, projects,
  arranging, audio and MIDI editing, devices, rendering, and settings.
- [`docs/02_extensions/`](docs/02_extensions/) — extension quickstart, package
  format, device API, panel API, and distribution.
- [`docs/03_development/`](docs/03_development/) — architecture, code structure,
  audio engine, editing, testing, roadmap, and crate reference.

Each crate has its own `README.md` describing its exact responsibility and
boundaries. Run `mdbook build` from the repository root to build the book.

## Inspiration

- [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — CLAP/VST3/LV2/AU plugin loading.
- [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — Bitwig-inspired UI/workflow ideas.

## License

Notwig is licensed under the [Mozilla Public License](LICENSE).
