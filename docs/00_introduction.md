# Introduction

Notwig is an open-source digital audio workstation for arranging, editing, and
mixing MIDI and audio. It is intentionally small: a focused set of features
designed to be learned quickly, extended safely, and kept correct in real time.

Notwig runs on macOS today and also builds for the browser with a reduced
feature set. It is not a Bitwig clone, a drop-in replacement, or a
feature-parity project. Bitwig is a reference for interaction and workflow
ideas.

## Current scope

Notwig can:

- create, save, and reopen directory-based project bundles;
- import WAV, AIFF, and MP3 audio, then arrange audio and MIDI clips;
- edit audio clip events, MIDI notes, and time selections;
- host stock Tool, EQ, and Chain devices plus VST3, AU, and WASM extension
  devices;
- mix tracks with volume, pan, mute, and solo, with latency compensation;
- loop playback and render an offline mixdown to several audio formats.

Notwig cannot record audio or MIDI, monitor live input, edit automation, warp
audio, or run on Windows and Linux yet. The [Roadmap](03_development/06_roadmap.md)
records the exact status of every feature.

## Documentation paths

This book has three audiences. Read the path that matches what you want to do.

### Using Notwig

Start with [Getting Started](01_users/00_getting_started.md). The user guide
covers the workspace, projects, tracks and mixing, the arranger, audio and MIDI
editing, devices, rendering, and settings. It explains what the application
does without assuming knowledge of the Rust code.

### Building an extension

Start with the [Extension Overview](02_extensions/00_overview.md) and the
[Quickstart](02_extensions/01_quickstart.md). Extensions are core WebAssembly
modules written in Rust against `notwig-ext-api`. They add devices with their
own DSP and custom panels. The guide covers the package format, the device
trait, the immediate-mode panel API, host limits, testing, and distribution.

### Developing Notwig

Start with [Architecture](03_development/00_architecture.md), then
[Code Structure](03_development/01_code_structure.md). The development section
describes the layering rules, state flow, audio thread contract, editing
algebra, test strategy, and roadmap. These pages are the design contract for
the codebase; when a design changes, update the page that describes it.

## Documentation rules

- Keep pages task-focused. A user page explains how to use a feature; a
  development page explains why the code is shaped the way it is.
- Prefer concrete behavior over aspiration. Mark planned work as planned.
- Update the relevant page in the same change that alters behavior, a public
  API, or a boundary between crates.
- Keep links relative to the current page and register new pages in
  [`SUMMARY.md`](SUMMARY.md).
