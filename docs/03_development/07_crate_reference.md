# Crate Reference

This page is a quick map of the workspace crates. Each crate also has a
`README.md` with its own module table and boundaries at `crates/<name>/README.md`.

## Workspace crates

| Crate | Owns | Depends on |
| --- | --- | --- |
| `notwig-core` | Domain document, IDs, time, tracks, clips, command protocol, editing algebra | No workspace crates |
| `notwig-project` | Project bundles, manifests, autosave/recovery, audio decode/resample/export | `notwig-core` |
| `notwig-dsp` | Device and parameter contracts, registry, graph compilation, stock devices | `notwig-core` |
| `notwig-ext-api` | Extension SDK, render-neutral draw/panel types, manifests, ABI helpers | `notwig-core`, `notwig-dsp`, `notwig-ext-macros` |
| `notwig-ext-macros` | `#[notwig_extension]` proc macro that generates wasm exports | Proc-macro support only |
| `notwig-ext` | Extension discovery, wasmtime host, WASM DSP, panel host | `notwig-core`, `notwig-dsp`, `notwig-ext-api` |
| `notwig-ext-devices` | Limiter example device and stock WASM extension | `notwig-core`, `notwig-dsp`, `notwig-ext-api` |
| `notwig-engine` | Real-time playback, runtime plans, scheduling, transport, meters | `notwig-core`, `notwig-dsp`, `notwig-plugins` |
| `notwig-plugins` | Plugin discovery/cache, host adapters, instance and editor lifecycle | `notwig-core`, `notwig-dsp` |
| `notwig-ui` | `egui` shell, views, editors, interaction, intents, runtime projection | `notwig-core`, `notwig-project`, `notwig-dsp`, `notwig-ext-api` |
| `notwig-app` | Composition, workspace policy, audio I/O, thread wiring, dialogs, history | UI, project, engine, plugins, DSP, extensions |
| `notwig-web` | Browser entry point, wasm-bindgen handle, WebRunner startup | `notwig-app` on wasm targets |

`notwig-rack` is a separate Cargo workspace under `crates/notwig-rack/`. It
vendors the VST3, AU, CLAP, and VST2 wrapper crates used by `notwig-plugins`. It
is excluded from the main workspace and its lint policy.

## Where to make a change

| Change | Start here |
| --- | --- |
| Document field, command, validation, edit algebra | `notwig-core` |
| Bundle format, asset import/export, autosave | `notwig-project` |
| Device contract, stock device, DSP graph | `notwig-dsp` |
| Extension API, draw commands, ABI wire types | `notwig-ext-api` |
| Extension loading, trust levels, WASM device runtime | `notwig-ext` |
| Playback, scheduling, transport, latency compensation | `notwig-engine` |
| VST3/AU discovery and hosting | `notwig-plugins` |
| View, widget, gesture, editor UI | `notwig-ui` |
| Intent handling, workspace policy, audio device, dialogs | `notwig-app` |
| Browser startup, OPFS storage, WebAudio gate | `notwig-app::web`, `notwig-web` |

## Crate documents

- `crates/notwig-core/README.md`
- `crates/notwig-project/README.md`
- `crates/notwig-dsp/README.md`
- `crates/notwig-ext-api/README.md`
- `crates/notwig-ext-macros/README.md`
- `crates/notwig-ext/README.md`
- `crates/notwig-ext-devices/README.md`
- `crates/notwig-engine/README.md`
- `crates/notwig-plugins/README.md`
- `crates/notwig-ui/README.md`
- `crates/notwig-app/README.md`
- `crates/notwig-web/README.md`
- `crates/notwig-rack/*/README.md`

## Development documents

- [Architecture](00_architecture.md) — layers, state flow, threads, real-time
  rules.
- [Code Structure](01_code_structure.md) — module map, extension recipes,
  structural hotspots.
- [Audio Engine](02_audio_engine.md) — control exchange and prepared runtime
  swaps.
- [Editing](03_editing.md) — shared editing algebra and adapter contract.
- [Editing Testing](04_editing_testing.md) — test strategy for editing.
- [Testing](05_testing.md) — coverage map and test gaps.
- [Roadmap](06_roadmap.md) — implemented, partial, planned, and dropped scope.
