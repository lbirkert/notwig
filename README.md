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

## Current features

### Projects

- Directory-based `*.notwig` project bundles with a versioned JSON manifest.
- Project sample rate selection (44.1, 48, and 96 kHz templates).
- Save, Save As, autosave snapshots, and recovery detection.
- Undo/redo (200 steps) with cheap non-structural undo for mixer and parameter changes.
- Offline mixdown to 32-bit float WAV, including range rendering.

### Arranger

- Audio, instrument, group, bus FX, and master tracks.
- Track rename, color, comment, mute/solo, volume, pan, grouping, and duplication.
- MIDI and audio clips on the linear timeline.
- Clip move, trim, split, duplicate, mute, and manual clip bounds.
- Object selection and time-range selection with copy, cut, paste, delete, duplicate, and move.
- 1/16 snap by default; hold Alt while dragging to bypass snapping.
- Audio drag-and-drop onto tracks, new tracks, or the clip editor.
- Pinned master track and fit-arrangement navigation.

### Audio clip editor

- Bottom panel for audio clips with a waveform view and event (slice) editing.
- Move, trim, split, duplicate, and delete audio events.
- Add audio events by dropping files into the clip editor.
- Shared editing algebra with the arranger, including clipboard and undo support.

### Device and information panels

- Device chains with stock EQ, Tool, and Chain devices.
- VST3 and AU hosting on macOS, including native plugin editor windows.
- Device browser, parameter controls, bypass, removal, and plugin reload.
- Info panel for tracks, devices, MIDI clips, audio clips, files, and time selections.
- Analysis views for EQ curves and meters.

### Audio and transport

- Transport play, pause, stop, loop, tempo, time signature, play-start, and playhead.
- Audio device selection (driver, device, sample rate, and block size).
- DSP load analysis window with the last 200 callback loads, average/max load,
  deadline, period jitter, and output latency.
- Engine builds happen off the UI thread; structural changes swap in a rebuilt plan.

### Plugins

- In-process VST3 and AU hosting through `truce-rack` on macOS.
- Background plugin discovery with a persistent cache.
- VST2 is reserved in the plugin model but not currently hosted.

## Status

Notwig is an early-stage prototype. It is playback/edit only:

- No audio or MIDI recording, live input, or monitoring.
- No MIDI note editor yet; MIDI clips can be created and edited at the clip level.
- No Clip Launcher, scenes, comping, takes, or audio warping.
- No tempo/time-signature automation, modulation runtime, or macros runtime.
- macOS is the only actively supported platform.

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
- [`docs/FEATURES.md`](docs/FEATURES.md) — current user-facing features and where they live.
- [`docs/EDITING.md`](docs/EDITING.md) — editing algebra and adapter contracts.
- [`docs/EDITING_TESTING.md`](docs/EDITING_TESTING.md) — testing strategy for edit operations.

Each crate has its own `README.md` describing its exact responsibility and
boundaries.

## Inspiration

- [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — VST3/AU plugin loading.
- [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — Bitwig-inspired UI/workflow ideas.

## License

Notwig is licensed under the [Mozilla Public License](LICENSE).
