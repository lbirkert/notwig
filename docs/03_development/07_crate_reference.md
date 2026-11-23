# Crate Reference

This page is a quick map of the workspace crates. Each crate also has a
`README.md` with its own module table and boundaries at `crates/<name>/README.md`.

## Workspace crates

| Crate | Owns | Depends on |
| --- | --- | --- |
| `notwig-core` | Domain document, IDs, time, tracks, clips, command protocol, editing algebra, device contracts, plan compilation, stock devices | No workspace crates |
| `notwig-project` | Project bundles, manifests, autosave/recovery, audio probe/decode/resample/export | `notwig-core` |
| `notwig-ext-api` | Extension SDK, guest panel wire types/runner, note events, manifests, ABI helpers | `notwig-core`, `notwig-ext-macros` |
| `notwig-ext-macros` | `#[notwig_extension]` proc macro that generates wasm exports | Proc-macro support only |
| `notwig-ext` | Extension discovery, wasmtime host, WASM DSP, panel host | `notwig-core`, `notwig-ext-api` |
| `notwig-ext-devices` | Limiter, gate, synth, and gain example devices using shared UI components | `notwig-core`, `notwig-ui`, `notwig-ext-api` |
| `notwig-ext-cli` | Extension scaffolding (`new`/`init`) and target-specific ZIP packaging | `notwig-ext-api` |
| `notwig-engine` | Real-time playback, runtime plans, scheduling, transport, meters | `notwig-core`, `notwig-plugins` |
| `notwig-plugins` | Plugin discovery/cache, host adapters, instance and editor lifecycle | `notwig-core` |
| `notwig-ui` | `egui` shell, views, editors, interaction, intents, runtime projection, shared extension components | `notwig-core`, `notwig-project`, `notwig-ext-api` |
| `notwig-app` | Composition, workspace policy, audio I/O, thread wiring, dialogs, history | UI, project, core/DSP, engine, plugins, extensions |
| `notwig-web` | Browser entry point, wasm-bindgen handle, WebRunner startup | `notwig-app` on wasm targets |

`notwig-rack` is a separate Cargo workspace under `crates/notwig-rack/`. It
vendors the VST3, VST2, CLAP, LV2, AU v2, and AU v3 wrapper crates used by
`notwig-plugins`. It is excluded from the main workspace and its lint policy.

## Where to make a change

| Change | Start here |
| --- | --- |
| Document field, command, validation, edit algebra | `notwig-core` |
| Bundle format, asset import/export, autosave | `notwig-project` |
| Device contract, stock device, DSP graph | `notwig-core::dsp` |
| Extension API, guest panel frame wire types, ABI wire types | `notwig-ext-api` |
| Extension loading, target selection, WASM/native device runtime | `notwig-ext/notwig-ext` |
| Extension scaffolding and package ZIPs | `notwig-ext/notwig-ext-cli` |
| Playback, scheduling, transport, latency compensation | `notwig-engine` |
| VST3/VST2/CLAP/LV2/AU/AU3 discovery and hosting | `notwig-plugins` |
| View, widget, gesture, editor UI | `notwig-ui` |
| Intent handling, workspace policy, audio device, dialogs | `notwig-app` |
| Browser startup, OPFS `Projects` folder storage, WebAudio gate | `notwig-app::web`, `notwig-web` |

## Crate documents

- `crates/notwig-core/README.md`
- `crates/notwig-project/README.md`
- `crates/notwig-ext/README.md` (subsystem overview)
- `crates/notwig-ext/notwig-ext-api/README.md`
- `crates/notwig-ext/notwig-ext-macros/README.md`
- `crates/notwig-ext/notwig-ext/README.md`
- `crates/notwig-ext/notwig-ext-devices/README.md`
- `crates/notwig-ext/notwig-ext-cli/README.md`
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
- [Audio Engine](02_audio_engine.md) — control exchange, prepared-runtime swaps,
  monitoring latency, rings, and recording capture.
- [Editing](03_editing.md) — shared editing algebra and adapter contract.
- [Editing Testing](04_editing_testing.md) — test strategy for editing.
- [Testing](05_testing.md) — coverage map and test gaps.
- [Roadmap](06_roadmap.md) — implemented, partial, planned, and dropped scope.
- [DSP Implementation and Performance](08_dsp_performance.md) — stock DSP
  internals, benchmarks, and optimization notes.
- [Audio Import and Analysis Performance](09_audio_import_performance.md) —
  probing, decoding, resampling, and import cost.
- [Waveform Rendering](10_waveform_rendering.md) — waveform data model, level
  selection, and render modes.
