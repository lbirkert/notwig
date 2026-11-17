# Crate Responsibility Contracts

## 1. `notwig-core` (`crates/notwig-core`)
- **Owns:** canonical `S_doc` types, IDs, `DeviceInstance`/`DeviceChain`/`AssetIndex`/`AssetEntry` schemas, tracks/sequencer/clip/param/routing schemas, validation and `apply(doc,cmd)->(doc',delta,engineMsgs)`, projections, and cross-boundary protocol types (`Command`, `StateDelta/QueryResult`, `EngineMsg`, `RtEvent`) in v0.
- **Depends on:** none (workspace).
- **Must not:** IO, UI, RT audio processing, plugin hosting, or ID allocation (IDs are caller-supplied).

## 2. `notwig-project` (`crates/notwig-project`)
- **Owns:** project lifecycle, `*.notwig/` bundle read/write, manifest/schema/migrations, autosave/recovery, asset IO/ingest (WAV/AIFF decode + offline resample), relink, export coordination.
- **Depends on:** `notwig-core`.
- **Must not:** UI logic or RT processing; all IO off RT thread.

## 3. `notwig-dsp` (`crates/notwig-dsp`)
- **Owns:** `DeviceSpec`/`ParamSpec`/port/signal contracts, registry resolution, graph/compiled-plan structures, stock FX device definitions, Faust compilation/cache abstractions, RT-safe traits/interfaces.
- **Depends on:** `notwig-core`.
- **Must not:** run audio callback, own persisted instance state, or persist latency.

## 4. `notwig-engine` (`crates/notwig-engine`)
- **Owns:** RT/offline execution, plan application, scheduler, transport/playback (authoritative playhead), PDC/latency measurement, metering/analysis taps, `S_rt`, engine<->control queues.
- **Depends on:** `notwig-core`, `notwig-dsp`, `notwig-plugins`.
- **Must not:** mutate `S_doc` or perform alloc/lock/IO/compile/UI in RT callback.

## 5. `notwig-plugins` (`crates/notwig-plugins`)
- **Owns:** format-agnostic hosting adapters (v0 VST3/AU primary, VST2 fallback) including hosted instruments (no stock instrument), discovery/scan/cache/blacklist, instantiate/load, param/state/preset mapping, editor lifecycle hooks, latency/capability reporting.
- **May depend on:** `truce-rack` (VST3/AU/CLAP loading) behind the `HostAdapter`, subject to license/API/RT-safety review.
- **Depends on:** `notwig-dsp`, `notwig-core`.
- **Must not:** scan/load/editor on RT thread; leak format-specific APIs or `truce-rack` types across the boundary.

## 6. `notwig-ui` (`crates/notwig-ui`)
- **Owns:** `egui`+`wgpu` shell/views/primitives, settings/preferences and notification surfaces, `S_ui`, projections of `S_doc/S_rt`, interaction/command emission, device and generic plugin UIs.
- **Depends on:** `notwig-core`, `notwig-project`, `notwig-dsp`.
- **Must not:** own canonical state, depend on engine RT internals, or block frames on heavy work.

## 7. `notwig-app` (`crates/notwig-app`)
- **Owns:** entrypoint, subsystem wiring, thread/runtime setup, channel plumbing, startup/shutdown, global settings/config/logging.
- **Depends on:** all other workspace crates as needed.
- **Must not:** contain reusable domain/DSP logic belonging in lower crates.

## 8. Interface Rules
- Shared boundary types live in lowest common crate (protocol in `notwig-core` for v0).
- Minimal public APIs; internal modules private by default.
- See `plan/crates/<crate>/` for detailed contracts.
