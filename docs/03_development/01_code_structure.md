# Code structure

This page maps the repository and describes where different kinds of changes
belong. It is intentionally practical: use it to find the right crate, module,
and test layer before editing.

## Repository layout

```text
.
  Cargo.toml                 workspace, dependencies, lint policy
  book.toml                  mdbook config for the GitHub Pages docs
  README.md                  project overview and build instructions
  AGENTS.md                  contributor/agent workflow and code style
  scripts/                   extension package/index build scripts
  .github/workflows/         CI, including GitHub Pages publishing
  docs/                      mdBook sources, ordered by audience
  web/                       static browser host page and styles
  plan/                      scratch/planning notes (not part of the stable docs)
  crates/
    notwig-core/
      src/dsp/              device contracts, plan compilation, stock devices
    notwig-project/
    notwig-ext/
      notwig-ext-api/
      notwig-ext-macros/
      notwig-ext/              host runtime
      notwig-ext-devices/
      notwig-ext-example/
      cargo-notwig-ext/
    notwig-engine/
    notwig-plugins/
    notwig-ui/
    notwig-app/
    notwig-web/              wasm-bindgen browser entry point
    notwig-rack/             vendored VST3/VST2/CLAP/LV2/AU/AU3 host crates
                             (separate Cargo workspace, excluded from lints)
```

Every crate has a `README.md` describing its purpose and boundaries. The
workspace enforces `#![forbid(unsafe_code)]` and a strict Clippy policy.

## Crate dependency graph

```text
notwig-web -> notwig-app (wasm target only)
notwig-app -> notwig-ui, notwig-core, notwig-project,
              notwig-engine, notwig-plugins,
              notwig-ext, notwig-ext-api
notwig-ui -> notwig-core, notwig-project, notwig-ext-api
notwig-ext -> notwig-core, notwig-ext-api
notwig-ext-api -> notwig-core, notwig-ext-macros
notwig-ext-macros -> proc-macro support only
notwig-ext-devices -> notwig-core, notwig-ui, notwig-ext-api
notwig-ext-example -> notwig-core, notwig-ext-api
                          (core/ui without default features)
cargo-notwig-ext -> notwig-ext-api
notwig-engine -> notwig-core, notwig-plugins
notwig-plugins -> notwig-core
notwig-project -> notwig-core
notwig-core -> (no workspace dependencies)
```

`crates/notwig-ext/` groups the extension SDK, host, examples, and CLI like
`crates/notwig-rack/` groups the plugin-host crates. The group folder has its own
README but is not a crate.

The graph must stay acyclic. Shared boundary types belong in the lowest crate
that needs them; most cross-layer types live in `notwig-core`.

## Module map

### `notwig-core` — domain and protocol

| Module | Responsibility |
|---|---|
| `domain.rs` | `ProjectDoc`, metadata, assets, devices, chains, transport, validation |
| `ids.rs` | Typed opaque IDs (`Id<T>`, `TrackId`, `ClipId`, ...) |
| `time.rs` | Musical/audio time, ranges, tempo map, units, colors, schema version |
| `tracks.rs` | Track kinds, routing, mixer, track forest/projections |
| `sequencer.rs` | Arranger clips, MIDI/audio payloads, notes, audio slices |
| `protocol/` | Command, delta, query, plan, schedule, engine-message, and runtime-event schemas in focused modules |
| `apply/` | Pure-ish command application: dispatch, transport, tracks, clips, notes, devices, assets, audio math |
| `edit/` | Editor-agnostic editing algebra (`algebra/`: materialize, delete, paste, move, trim) and arranger adapter |
| `dsp/` | Device contracts, parameters, registry, plan compilation, extension container processing, stock devices (`stock/`: specs, tool, eq, chain, biquad), factories, monitoring |

### `notwig-project` — persistence and assets

| Module | Responsibility |
|---|---|
| `bundle.rs` | `*.notwig` save/open/autosave/recovery |
| `manifest.rs` | Versioned JSON manifest and load issues |
| `assets.rs` | Audio decode, import, content-addressed persistence, float WAV writing |
| `probe.rs` | Header-only duration, sample-rate, and channel probe for import previews |
| `resample.rs` | `rubato`-based offline sample-rate conversion |
| `export.rs` | WAV/AIFF/FLAC/MP3/OGG export with bit depth, dithering, and normalization |
| `fsutil.rs` | Atomic file/JSON writes and directory creation |
| `error.rs` | `ProjectError` |

### `notwig-core::dsp` — device contracts and plan compilation

| Module | Responsibility |
|---|---|
| `devices.rs` | `DeviceProcessor`, `DeviceSpec`, ports, note events, process context |
| `params.rs` | Parameter specs, ranges, smoothing, normalized mapping |
| `registry.rs` | Type ID to device spec/processor resolution |
| `graph.rs` | Compile `ProjectDoc` into a `CompiledPlan` (tracks, sends, chains, latency) |
| `container.rs` | Host-owned container processor for extension stages, child chains, and wet blending |
| `stock/` | Stock device facade; specs, Tool, EQ, Chain, and shared biquad math |
| `factory.rs` | `DeviceFactory` trait, stock factory, and composite factory |
| `monitor.rs` | Lock-free per-device audio monitor ring for plugin editor meters |
| `smoothing.rs` | Parameter smoothing primitives |

### `notwig-ext-api` — extension SDK and panel transport

ABI v5 adds extension container metadata; the guest panel remains the only extension panel path. Extensions run a private
`egui::Context` and return validated meshes, texture deltas, and host actions.

| Module | Responsibility |
|---|---|
| `geometry.rs` | Renderer-neutral `Color` used by theme snapshots |
| `cursor.rs` | Neutral cursor request enum |
| `panel.rs` | `PanelAction`, `DevicePanelHost` |
| `context.rs` | `DeviceDrawContext` (panel input and device meters) and theme snapshot |
| `manifest.rs` | extension manifest v1/v2, ordered entrypoints, capabilities, devices, and contribution declarations |
| `target.rs` | canonical target names, aliases, and host support checks |
| `status.rs` | `ExtensionStatus`, `ExtensionInfo`, settings |
| `config.rs` | Versioned user config, default index URL, legacy migration |
| `index.rs` | Extension index entry/device/url types and validation |
| `note.rs` | Extension note events and their fixed-size ABI wire format |
| `panel_frame/` | Guest panel wire types, validation, frame runner, and theme bridge |
| `command.rs` | Command contract, bounded context, host edit/UI families, declarative `when` evaluation, and `CommandExtension` |
| `extension.rs` | `DeviceExtension`, `CommandExtension`, `Extension`, registration, note delivery, command execution, and per-device panel draw dispatch |
| `abi.rs` | ABI version (`5`), status codes, pointer/length packing, helper re-exports |
| `abi/heap.rs` | Stable bump heap for host inputs and audio buffers |
| `abi/store.rs` | Handle-keyed `DeviceStore<T>` with per-instance locks |
| `abi/pointers.rs` | Safe wrappers for WASM offsets and native pointers to strings, param IDs, and audio slices |

### `notwig-ext-macros` — extension ABI generation

| Module | Responsibility |
|---|---|
| `lib.rs` | `#[notwig_extension]` attribute macro; generates WASM exports or the native shared-library ABI around the entrypoint, including command execution |

### `notwig-ext` — extension host

| Module | Responsibility |
|---|---|
| `discovery.rs` | user-directory scan, manifest parse/validate, first supported entrypoint |
| `archive.rs` | shared ZIP/file-tree validation, safe relative paths, manifest and size limits |
| `abi.rs` | wasmtime engines/stores, host imports, typed exports, memory checks |
| `native.rs` | `libloading` shared-library loader and native instance ABI |
| `instance.rs` | WASM/native instance enum with common device and command calls |
| `compiled.rs` | loaded extension records, runtime kind, and device-type lookup |
| `host.rs` | `ExtensionHost`: load/compile, specs, command descriptors/execution, factory, panel host, infos |
| `device.rs` | `ExtensionDeviceFactory`, WASM and native `DeviceProcessor`s |
| `install.rs` | filesystem install/replace and uninstall using validated archives |
| `panel.rs` | cached UI instance implementing `DevicePanelHost` |
| `web/` | browser host, package loading, WASM DSP, and panels |
| `error.rs` | `ExtensionError` |

### `cargo-notwig-ext` — extension scaffolding and packaging

| Module | Responsibility |
|---|---|
| `build/` | `source` discovery and Cargo metadata, `compile` per-target builds, `package` ZIP/manifest assembly, `paths` package path rules |
| `scaffold.rs` | `new`/`init`: Cargo.toml, `src/lib.rs`, source manifest, default icon |
| `target.rs` | Canonical target to Rust triple, artifact suffix, and entrypoint naming |
| `error.rs` | CLI errors |

The parent [`crates/notwig-ext/README.md`](../../crates/notwig-ext/README.md)
describes the extension subsystem and crate boundaries.

### `notwig-ext-example` — command API example

| Module | Responsibility |
|---|---|
| `lib.rs` | Humanize MIDI command and `#[notwig_extension]` registration |

### `notwig-ext-devices` — example extension devices

| Module | Responsibility |
|---|---|
| `limiter.rs` | Limiter `DeviceExtension` impl: DSP, spec, panel, meters, parameter IDs |
| `gate/` | Gate `DeviceExtension` impl split into `params`, `spec`, `panel`, and `dsp` modules |
| `synth/` | Polyphonic synth split into `params`, `spec`, `panel`, and `dsp` modules |
| `compressor.rs` | Stereo-linked compressor DSP and panel |
| `clipper.rs` | Soft clipper, tilt tone control, and transfer-curve panel |
| `delay.rs` | Stereo delay with crossfeed, damping, ducking, and width |
| `chorus.rs` | Stereo chorus/flanger with modulated delay lines |
| `transient.rs` | Attack/sustain transient shaper and envelope panel |
| `reverb.rs` | Stereo algorithmic reverb, Room/Hall/Plate types, wet FX chain, and panel |
| `util.rs` | Shared smoothing, delay-line, filter, conversion, and soft-clip helpers |
| `gain.rs` | Small guest-egui gain example using shared `notwig-ui` controls |
| `dynamics.rs` | Shared 50-frame peak/gain history and egui history-graph painter |
| `ui.rs` | Shared knob caption helper |
| `lib.rs` | `#[notwig_extension]` entrypoint registering all devices |

### `notwig-engine` — real-time execution

| Module | Responsibility |
|---|---|
| `engine.rs` | `Engine`, transport, plan/schedule application, block processing, control sync |
| `engine/metronome.rs` | Metronome state and click rendering |
| `engine/offline.rs` | Offline render entry points |
| `control.rs` | `EngineData`, `EngineControl`, `ControlHandle`, `PreparedRuntime`, `RuntimeLoader`, input routes |
| `input.rs` | Bounded monitoring and recording queues for per-track input capture |
| `runtime.rs` | Active-plan ownership, build orchestration, track/device runtime state |
| `runtime/clips.rs` | Audio clip and slice rendering |
| `runtime/events.rs` | MIDI event collection and note priming |
| `runtime/graph.rs` | Processing order, routing targets, and processor construction |
| `runtime/low_latency.rs` | Duplicate low-latency monitor chains and their device-block render path |
| `runtime/routing.rs` | Track gain/pan, main/send routing, and metering |
| `factory.rs` | Stock and plugin device factories |

### `notwig-plugins` — plugin hosting

| Module | Responsibility |
|---|---|
| `host.rs` | `PluginHost`, `HostAdapter`, and crash-isolating `PluginProbe` boundary |
| `adapters.rs` | VST3/VST2/CLAP/LV2/AU/AU3 adapters built on vendored `notwig-rack` |
| `cache.rs` | Persisted plugin scan cache |
| `types.rs` | Format-agnostic descriptors, metadata, states, capabilities, errors |
| `native_editor/` | Native plugin editor window lifecycle: editor/monitor pump, AppKit host, Space-key monitor, platform stub |
| `rack.rs` | Vendored `notwig-rack` integration details and latency reporting |

### `notwig-ui` — views and interaction

| Module | Responsibility |
|---|---|
| `views/shell/` | App bar, transport toolbar, status bar, layout, shortcuts, overlays, help, drag/drop |
| `views/arranger/` | Track lanes, clip interaction, navigation, drop preview, edit helpers; track split into header, lane, drop, menu |
| `views/info/` | Left info panel: track, device, MIDI clip, audio clip, files, time |
| `views/panel.rs` | Shared bottom-panel chrome and layout helpers |
| `views/audio_editor/` | Audio clip editor: state, actions, geometry, model, navigation, gain sub-editor; panel split into range selection, slice paint, slice gestures |
| `views/midi_editor/` | MIDI note editor: state, model, geometry, actions, navigation; panel split into paint, note drag, velocity lane, audition |
| `views/device_editor/` | Device panel, browser, drag/drop and insertion points, parameter widgets, EQ display, renderer dispatch; card/stock body/knob/bracket/plugin-kind split |
| `views/dsp.rs` | DSP load analysis window |
| `views/settings/` | Settings window/navigation plus audio, audio-channel, extension, and shortcut pages |
| `commands/` | Built-in command catalog, resolved command registry, chords, string-keyed bindings, and config |
| `state/` | Selection, viewport, notifications, open editor, edit focus |
| `views/render.rs` | Render mixdown window |
| `components/` | Extension-facing knobs, faders, meters, labels, sections, toggles, dropdowns, and formatters |
| `palette.rs` | Shared color constants for the app and guest components |
| `widgets/` | App-only draggable numbers, form helpers, brand assets, icons, theme |
| `widgets/snap.rs` | Shared `SnapSettings`/`SnapLevel` values, adaptive step selection, and snap math |
| `widgets/timeline/` | Shared beat strip (`beat_strip/`: interaction and paint), clip visuals, grid, snap controls, selection, drop preview, interaction math |
| `widgets/drop_target.rs` | Drag preview and drop-target resolution |
| `waveform.rs` | Signed min/max waveform pyramid, level-of-detail selection, sample-level drawing |
| `clipboard.rs` | Shared arranger/audio-clip clipboard payloads |
| `intent.rs` | `UiIntent` and drag/drop target types |
| `runtime.rs` | Read-only runtime projection (`RuntimeView`, meters, audio/DSP views) |

### `notwig-app` — composition and runtime policy

| Module | Responsibility |
|---|---|
| `lib.rs` | Shared native/browser library surface and module wiring |
| `main.rs` | Clap-based CLI entrypoint, demo/new/render/info/scan commands |
| `gui/` | `eframe` app facade plus intents, audio supervision, recording, render, dialogs, plugin windows, extensions, platform edges |
| `workspace/` | Project lifecycle facade plus command application, engine, persistence, transport, tracks, devices, device clipboard, clips, sample rate, render |
| `workspace/assets.rs` | Asset loading, import, relink, waveform pyramids, engine asset preparation |
| `workspace/recovery.rs` | Session lock lifecycle, autosave recovery, discard |
| `web/mod.rs` | Browser app construction |
| `web/storage.rs` | OPFS project and extension storage, project picker, and ZIP download |
| `clock.rs` | Native/browser clock selection |
| `engine_build.rs` | Async engine build request/queue/poll lifecycle; native wire-build preparation |
| `engine_process/` | Native engine child composition, worker/build lifecycle, IPC protocol, parent client, and editor hosting |
| `audio/` | `settings` for device/config policy, `snapshot` for telemetry, `stream` for CPAL callbacks, `sample_format` for format dispatch, and `service` for wiring |
| `audio_input/` | Shared input requests, CPAL input streams on native, Web Audio microphone capture in the browser, and input queue routing |
| `recording.rs` | Recording-session writer and capture-buffer lifecycle |
| `audio_config.rs` | Persisted audio driver/device/block-size, named-channel, and master-output config |
| `app_paths.rs` | Canonical per-user app, cache, and log directories |
| `plugins/mod.rs` | Target-selected plugin catalog exports |
| `plugins/native.rs` | Plugin catalog, scan subprocess, crash-isolated load probe, and cache glue |
| `plugins/web.rs` | Empty browser plugin catalog |
| `extensions/mod.rs` | Target-selected extension manager and events |
| `extensions/index.rs` | Shared index URL resolution, validation, and version deduplication |
| `extensions/native.rs` | Config/index fetch, install/uninstall jobs, wasmtime host lifecycle |
| `extensions/web.rs` | Index fetch, OPFS install/discovery, browser host lifecycle, and app sync |
| `shortcuts.rs` | User shortcut config load/save |
| `edit_lowering.rs` | Convert edit plans into `CommandPayload`s |
| `history.rs` | Undo/redo stack with structural/non-structural entries |
| `logging.rs` | `tracing` initialization, timestamped native app/engine logs, helper-process forwarding, and OPFS browser logs |

### `notwig-web` — browser entry

| Module | Responsibility |
|---|---|
| `lib.rs` | `wasm-bindgen` `WebApp` handle, `eframe::WebRunner` startup, audio gesture forwarding |

`notwig-web` is a `cdylib` and is empty on non-wasm targets. Its generated JS
package is copied under `book/app/pkg` by the Pages workflow.

## Tests

Integration tests live in `crates/*/tests/`, not inside `src/`.

| Area | Location |
|---|---|
| Domain apply/protocol | `crates/notwig-core/tests/domain_flow.rs`, `schedule_emission.rs` |
| Edit algebra/properties | `crates/notwig-core/tests/edit_algebra.rs` |
| MIDI note commands | `crates/notwig-core/tests/midi_notes.rs` |
| Time and IDs | `crates/notwig-core/tests/time.rs`, `ids.rs` |
| Project bundles, autosave, and recovery | `crates/notwig-project/tests/bundle.rs` |
| Audio probing, export, hashing, resampling | `crates/notwig-project/tests/probe.rs`, `export.rs`, `import_hash.rs`, `resample.rs` |
| Browser bundle/files and byte audio I/O | `crates/notwig-project/tests/browser_io.rs` |
| DSP plans and stock devices | `crates/notwig-core/tests/dsp_graph.rs`, `dsp_stock.rs` |
| Extension ABI, WASM devices, panels | `crates/notwig-ext/notwig-ext/tests/abi.rs` |
| Panel wire validation | `crates/notwig-ext/notwig-ext-api/tests/panel_wire.rs` |
| Panel bridge | `crates/notwig-ext/notwig-ext-api/tests/panel_bridge.rs` |
| Extension ABI helpers | `crates/notwig-ext/notwig-ext-api/tests/abi_helpers.rs` |
| Extension command contract and `when` parser | `crates/notwig-ext/notwig-ext-api/tests/command_contract.rs` |
| Host command resolution/execution | `crates/notwig-ext/notwig-ext/tests/commands.rs` |
| Command palette, registry, and string bindings | `crates/notwig-ui/tests/shortcuts.rs` |
| Extension command effect and undo | `crates/notwig-app/tests/workspace.rs` |
| Compressor, clipper, delay, chorus, transient, limiter, gate, reverb, synth, and gain example devices | `crates/notwig-ext/notwig-ext-devices/tests/` |
| Extension index and config | `crates/notwig-ext/notwig-ext-api/tests/index_config.rs` |
| Extension ZIP install | `crates/notwig-ext/notwig-ext/tests/install.rs` |
| Extension explorer UI | `crates/notwig-ui/tests/extension_settings.rs` |
| Built limiter/reverb/synth load through the host | `crates/notwig-ext/notwig-ext/tests/limiter_extension.rs` |
| Extension panel transport | `crates/notwig-ext/notwig-ext/tests/abi.rs` |
| Engine playback/rendering | `crates/notwig-engine/tests/render.rs` |
| Control exchange and prepared runtimes | `crates/notwig-engine/tests/control.rs` |
| MIDI preview note-offs | `crates/notwig-engine/tests/preview.rs` |
| Latency compensation | `crates/notwig-engine/tests/pdc.rs` |
| Loop wrapping and transport | `crates/notwig-engine/tests/transport.rs` |
| Plugin cache/types | `crates/notwig-plugins/tests/` |
| Arranger interaction and track moves | `crates/notwig-ui/tests/arranger.rs` |
| Timeline math, waveform mapping, trim | `crates/notwig-ui/tests/timeline.rs`, `clip_trim.rs` |
| Shell internals and editor exclusivity | `crates/notwig-ui/tests/shell_internal.rs` |
| View state and selection | `crates/notwig-ui/tests/state.rs` |
| Clipboard and edit helpers | `crates/notwig-ui/tests/edit.rs`, `clip_chrome.rs` |
| Widget math | `crates/notwig-ui/tests/widgets.rs` |
| Waveform pyramid | `crates/notwig-ui/tests/waveform.rs` |
| Device browser | `crates/notwig-ui/tests/device_browser.rs` |
| Extension device panel compositing | `crates/notwig-ui/tests/extension_panel.rs` |
| MIDI editor interaction | `crates/notwig-ui/tests/midi_editor.rs` |
| Peak hold and reset | `crates/notwig-ui/tests/track_info.rs` |
| Shortcut bindings and settings page | `crates/notwig-ui/tests/shortcuts.rs` |
| Workspace integration | `crates/notwig-app/tests/workspace.rs` |
| CLI demo flow | `crates/notwig-app/tests/cli.rs` |
| Undo/redo history | `crates/notwig-app/tests/history.rs` |
| MIDI note editing through the workspace | `crates/notwig-app/tests/midi_editor.rs` |
| Audio config persistence | `crates/notwig-app/tests/audio_config.rs` |
| Session lock and recovery | `crates/notwig-app/tests/recovery.rs` |
| Tempo, meters, and transport | `crates/notwig-app/tests/transport.rs` |

UI tests run `egui` headlessly with `Context::default()` and `RawInput`. Use
`notwig_ui::test_api` only from tests; it is `#[doc(hidden)]`.

## Extension recipes

### Add a document command

1. Add the variant to `CommandPayload` in `notwig-core/src/protocol/commands.rs`.
2. Update `requires_plan_rebuild` if the command changes graph topology.
3. Implement application in the matching `notwig-core/src/apply/` submodule; emit `EngineMsg` only
   when the engine needs to react.
4. Add a `Workspace` method in the matching `notwig-app/src/workspace/` submodule.
5. Add a `UiIntent` variant and handle it in `gui/intents.rs`.
6. Add tests at the apply layer and, if UI-driven, at the workspace layer.

### Add a stock device

1. Define the spec in `notwig-core/src/dsp/stock/specs.rs` and the processor in the matching stock submodule.
2. Register it in `stock_registry`.
3. Add parameters with sensible ranges/defaults and smoothing.
4. Add UI-specific presentation only if the generic device panel is insufficient.
5. Add DSP rendering tests in `crates/notwig-core/tests/dsp_stock.rs`.

### Add a keyboard shortcut

1. Add a `ShortcutCommand` variant and its `CommandSpec` entry in
   `notwig-ui/src/commands/`, including a stable id and defaults.
2. Handle the command in `notwig-ui/src/views/shell/` (or the focused editor) with
   `shortcuts.was_pressed(ctx, ShortcutCommand::YourCommand)`.
3. Emit a `UiIntent` or call the existing workspace/UI behavior.
4. Add the user-facing action to [`Roadmap`](06_roadmap.md) when it lands.
5. Add a config round-trip or capture test under `crates/notwig-ui/tests/`.

### Add a UI panel or window

1. Keep view code in `notwig-ui`.
2. Emit `UiIntent`; do not mutate `ProjectDoc` directly.
3. Put behavior in `Workspace` or the matching `gui/` submodule.
4. Reuse `widgets/`, `widgets/timeline/`, and `widgets/form.rs` helpers instead of local copies.
5. Add a headless render test in `crates/notwig-ui/tests/`.

### Add an extension device

1. Scaffold with `cargo notwig-ext new <path>` or `init`, or depend on
   `notwig-ext-api` directly; use `notwig-core` and `notwig-ui` without default
   features and build the crate as a `cdylib`/`rlib`.
2. Implement `DeviceExtension`: return a `DeviceSpec` from `spec`, draw with
   `draw`, keep DSP in `process` over interleaved `f32` audio, and handle
   `note_event` if the device declares a note input. Register the type with
   `#[notwig_extension]`.
3. In `draw`, use the shared `notwig-ui` components (`knob`, meters,
   sections, dropdowns), keep IDs stable with `ui.make_persistent_id(...)`, and
   emit parameter changes through `panel_frame::set_param_normalized` /
   `set_param_value`. The host validates the resulting `PanelAction`s against
   the device spec.
4. Ship a source `extension.json` with `api_version: 5`, no `entrypoints`, the
   `device`/`ui.panel` capabilities, and optional target-gated `includes`. Set
   `DeviceUiSpec::custom` to route the panel to the extension renderer. Build
   with `cargo notwig-ext build --entrypoint <target>` on each platform, then run
   `cargo notwig-ext build --package` to add entrypoints and bundle the ZIP.
5. Add frame tests in `crates/notwig-ext/notwig-ext-devices/tests/` and a host compositing
   test in `crates/notwig-ui/tests/`. See
   [Extension Overview](../02_extensions/00_overview.md) for the full package
   and panel guide.

### Add an audio feature

1. Decide whether it belongs to asset import, engine execution, or app audio I/O.
2. Keep file decoding and resampling in `notwig-project`.
3. Keep real-time processing in `notwig-engine` and device contracts in `notwig-core::dsp`.
4. Keep device/stream selection in `notwig-app/src/audio/settings.rs`, stream callbacks in `stream.rs`, snapshots in `snapshot.rs`, and service wiring in `service.rs`.
5. Follow the real-time rules in `AGENTS.md` and [`Architecture`](00_architecture.md).

## Known structural hotspots

These are the largest or most mixed modules. Split along concrete seams when
changing them; do not add unrelated responsibilities.

- `crates/notwig-ui/src/views/arranger/mod.rs` — track lanes, clip interaction,
  and timeline gestures are still coupled; it also rebuilds `TimelineGrid` four
  times per frame.
- `crates/notwig-engine/src/engine.rs` — control application and the real-time
  process loop are still large, but they are tightly coupled to private engine
  state. Split only if that area keeps changing.
- `crates/notwig-app/src/engine_process/worker.rs` — build, recording, control,
  and snapshot relay are related but still share one worker lifecycle.
- `crates/notwig-ext/notwig-ext/src/panel.rs` and `web/panel.rs` — panel
  instance caching and frame relay remain paired but separate per transport.
- `crates/notwig-core/src/apply/clips.rs` — `split_clip` and `edit_clip` remain
  the largest single command implementations.
- `crates/notwig-ui/src/views/shell/shortcuts.rs` and `top_bar.rs` — the
  shortcut router and transport bar are still large even after the file split.

Deferred structural work:

- Extract an `AudioSettingsDraft` type so driver/device/sample-rate reset policy
  is not spread across UI closures.
- Add a shared popup-window builder for settings, render, DSP, and future dialogs.
- Replace the arranger's repeated `TimelineGrid` construction with one shared
  grid per frame.

## Conventions

- Start with the narrowest visibility: private by default, `pub(crate)` for
  crate-wide helpers, `pub` only for intentional APIs.
- Prefer pure functions and explicit state transitions.
- Keep IDs caller-supplied; domain code does not allocate IDs or use randomness.
- Use ordered collections when iteration order affects behavior or persistence.
- Return typed errors; do not panic on recoverable input.
- Keep the real-time path free of allocation, blocking locks, I/O, plugin
  loading, and unbounded work.
- Reuse shared UI math and widgets rather than copying layout/interaction code.
- Update the relevant crate README or docs page when a boundary changes.
