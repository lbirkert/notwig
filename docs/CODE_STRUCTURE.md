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
  docs/                      authoritative documentation
  plan/                      scratch/planning notes (not part of the stable docs)
  crates/
    notwig-core/
    notwig-project/
    notwig-dsp/
    notwig-ext-api/
    notwig-ext-macros/
    notwig-ext/
    notwig-ext-devices/
    notwig-engine/
    notwig-plugins/
    notwig-ui/
    notwig-app/
    notwig-truce-rack/       vendored VST3/AU host crates (excluded from workspace lints)
```

Every crate has a `README.md` describing its purpose and boundaries. The
workspace enforces `#![forbid(unsafe_code)]` and a strict Clippy policy.

## Crate dependency graph

```text
notwig-app -> notwig-ui, notwig-core, notwig-project,
              notwig-engine, notwig-plugins, notwig-dsp, notwig-ext
notwig-ui -> notwig-core, notwig-project, notwig-dsp, notwig-ext-api
notwig-ext -> notwig-core, notwig-dsp, notwig-ext-api
notwig-ext-api -> notwig-core, notwig-dsp, notwig-ext-macros
notwig-ext-macros -> proc-macro support only
notwig-ext-devices -> notwig-core, notwig-dsp, notwig-ext-api
                          (core/dsp without default features)
notwig-engine -> notwig-core, notwig-dsp, notwig-plugins
notwig-plugins -> notwig-dsp, notwig-core
notwig-project -> notwig-core
notwig-dsp -> notwig-core
notwig-core -> (no workspace dependencies)
```

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
| `protocol.rs` | `CommandPayload`, `EngineMsg`, `RtEvent`, schedules, plan handles |
| `apply.rs` | Pure-ish command application: document in, document + engine messages out |
| `edit/` | Editor-agnostic editing algebra and arranger adapter |

### `notwig-project` — persistence and assets

| Module | Responsibility |
|---|---|
| `bundle.rs` | `*.notwig` save/open/autosave/recovery |
| `manifest.rs` | Versioned JSON manifest and load issues |
| `assets.rs` | Audio decode, offline resampling, import, WAV f32 export |
| `error.rs` | `ProjectError` |

### `notwig-dsp` — device contracts and plan compilation

| Module | Responsibility |
|---|---|
| `devices.rs` | `DeviceProcessor`, `DeviceSpec`, ports, note events, process context |
| `params.rs` | Parameter specs, ranges, smoothing, normalized mapping |
| `registry.rs` | Type ID to device spec/processor resolution |
| `graph.rs` | Compile `ProjectDoc` into a `CompiledPlan` (tracks, sends, chains, latency) |
| `stock.rs` | Stock EQ, Tool, and Chain definitions/processors |
| `smoothing.rs` | Parameter smoothing primitives |

### `notwig-ext-api` — extension SDK and render-neutral drawing

| Module | Responsibility |
|---|---|
| `geometry.rs` | `Point`, `Rect`, `Color`, `Stroke`, text alignment |
| `draw.rs` | `DrawList`/`DrawCommand` and validation limits |
| `panel.rs` | `PanelDraw`/`PanelCommand`, `DevicePanelHost` |
| `context.rs` | device panel context and theme snapshot |
| `manifest.rs` | extension manifest/capability/device declaration types |
| `status.rs` | `TrustLevel`, `ExtensionStatus`, `ExtensionInfo`, settings |
| `config.rs` | Versioned user config, default index URL, legacy migration |
| `index.rs` | Extension index entry/device types and validation |
| `extension.rs` | `DeviceExtension`, `Extension`, registration and ABI-facing dispatch |
| `abi.rs` | ABI version, status codes, pointer/length packing, helper re-exports |
| `abi/heap.rs` | Stable bump heap for host inputs and audio buffers |
| `abi/store.rs` | Handle-keyed `DeviceStore<T>` with const construction |
| `abi/pointers.rs` | Safe wrappers for reading strings, param IDs, and audio slices |

### `notwig-ext-macros` — extension ABI generation

| Module | Responsibility |
|---|---|
| `lib.rs` | `#[notwig_extension]` attribute macro; generates wasm exports around the entrypoint |

### `notwig-ext` — WASM extension host

| Module | Responsibility |
|---|---|
| `discovery.rs` | user-directory scan, manifest parse/validate, module size checks |
| `abi.rs` | wasmtime engines/stores, host imports, typed exports, memory checks |
| `compiled.rs` | loaded extension records and device-type lookup |
| `host.rs` | `ExtensionHost`: compile, specs, factory, panel host, infos |
| `device.rs` | `ExtensionDeviceFactory`, WASM `DeviceProcessor`, draw-only silence |
| `install.rs` | ZIP validation/extraction, safe install/replace, uninstall |
| `panel.rs` | cached UI instance implementing `DevicePanelHost` |
| `error.rs` | `ExtensionError` |

### `notwig-ext-devices` — example extension devices

| Module | Responsibility |
|---|---|
| `limiter.rs` | Limiter `DeviceExtension` impl: DSP, spec, panel, parameter IDs |
| `lib.rs` | `#[notwig_extension]` entrypoint registering `LimiterDevice` |

### `notwig-engine` — real-time execution

| Module | Responsibility |
|---|---|
| `engine.rs` | Transport, plan/schedule application, block processing, offline render |
| `runtime.rs` | Active plan instances, track/device runtime state, event collection |
| `factory.rs` | Stock and plugin device factories |

### `notwig-plugins` — plugin hosting

| Module | Responsibility |
|---|---|
| `host.rs` | `PluginHost` and `HostAdapter` boundary |
| `adapters.rs` | VST3/AU adapters built on vendored `truce-rack` |
| `cache.rs` | Persisted plugin scan cache |
| `types.rs` | Format-agnostic descriptors, states, capabilities, errors |
| `native_editor.rs` | Native plugin editor window lifecycle |
| `rack.rs` | Vendored `truce-rack` integration details and latency reporting |

### `notwig-ui` — views and interaction

| Module | Responsibility |
|---|---|
| `views/shell.rs` | Top bar, transport, status bar, layout, shortcuts, notifications |
| `views/arranger/` | Track lanes, clip interaction, track headers, navigation, drop preview, edit helpers |
| `views/info/` | Left info panel: track, device, MIDI clip, audio clip, files, time |
| `views/panel.rs` | Shared bottom-panel chrome and layout helpers |
| `views/audio_editor/` | Audio clip editor: state, panel, actions, geometry, model, navigation |
| `views/midi_editor/` | MIDI note editor: state, model, geometry, actions, navigation, panel |
| `views/device_editor/` | Device panel, browser, parameter widgets, EQ display |
| `views/dsp.rs` | DSP load analysis window |
| `views/settings/mod.rs` | Settings window and category navigation |
| `views/settings/shortcuts.rs` | Shortcut editor page and key capture |
| `commands.rs` | Shortcut command catalog, chords, bindings, and config |
| `views/render.rs` | Render mixdown window |
| `widgets/` | Knobs, faders, draggable numbers, form helpers, icons, theme |
| `widgets/timeline/` | Shared beat strip, clip visuals, waveform, grid, selection, drop preview, interaction math |
| `widgets/drop_target.rs` | Drag preview and drop-target resolution |
| `state.rs` | Selection, viewport, open editor, notifications, edit focus |
| `intent.rs` | `UiIntent` and drag/drop target types |
| `runtime.rs` | Read-only runtime projection (`RuntimeView`, meters, audio/DSP views) |

### `notwig-app` — composition and runtime policy

| Module | Responsibility |
|---|---|
| `main.rs` | CLI entrypoint, demo/new/render/info/scan commands |
| `gui.rs` | `eframe` app, intent handling, dialogs, plugin windows, runtime projection |
| `workspace/mod.rs` | Project lifecycle, command application, undo/redo, tracks, devices, transport |
| `workspace/assets.rs` | Asset loading, import, relink, peaks, engine asset preparation |
| `workspace/recovery.rs` | Session lock lifecycle, autosave recovery, discard |
| `engine_build.rs` | Async engine build request/queue/poll lifecycle |
| `audio.rs` | CPAL audio service, device options, snapshot, DSP timing |
| `audio_config.rs` | Persisted audio driver/device/block-size config |
| `plugins.rs` | Plugin catalog/cache/scanner subprocess glue |
| `extensions.rs` | Extension config/index fetch, install/uninstall jobs, host lifecycle, app sync |
| `shortcuts.rs` | User shortcut config load/save |
| `edit_lowering.rs` | Convert edit plans into `CommandPayload`s |
| `history.rs` | Undo/redo stack with structural/non-structural entries |
| `logging.rs` | `tracing` initialization |

## Tests

Integration tests live in `crates/*/tests/`, not inside `src/`.

| Area | Location |
|---|---|
| Domain apply/protocol | `crates/notwig-core/tests/` |
| Edit algebra/properties | `crates/notwig-core/tests/edit_algebra.rs` |
| Project bundles and assets | `crates/notwig-project/tests/` |
| DSP plans and stock devices | `crates/notwig-dsp/tests/` |
| Extension ABI, WASM devices, panels | `crates/notwig-ext/tests/` |
| Extension draw/panel validation | `crates/notwig-ext-api/tests/` |
| Extension ABI helpers | `crates/notwig-ext-api/tests/abi_helpers.rs` |
| Limiter example device | `crates/notwig-ext-devices/tests/` |
| Extension index and config | `crates/notwig-ext-api/tests/index_config.rs` |
| Extension ZIP install | `crates/notwig-ext/tests/install.rs` |
| Extension explorer UI | `crates/notwig-ui/tests/extension_settings.rs` |
| Built limiter loads through the host | `crates/notwig-ext/tests/limiter_extension.rs` |
| Engine playback/rendering | `crates/notwig-engine/tests/` |
| Plugin cache/types | `crates/notwig-plugins/tests/` |
| UI headless rendering, gestures, state | `crates/notwig-ui/tests/` |
| Extension device panels | `crates/notwig-ui/tests/extension_panel.rs` |
| MIDI editor interaction | `crates/notwig-ui/tests/midi_editor.rs` |
| Clip visuals and trim | `crates/notwig-ui/tests/clip_trim.rs` |
| Peak hold and reset | `crates/notwig-ui/tests/track_info.rs` |
| Shortcut bindings and settings page | `crates/notwig-ui/tests/shortcuts.rs` |
| Workspace and history integration | `crates/notwig-app/tests/` |
| MIDI note editing through the workspace | `crates/notwig-app/tests/midi_editor.rs` |
| Audio config persistence | `crates/notwig-app/tests/audio_config.rs` |
| Session lock and recovery | `crates/notwig-app/tests/recovery.rs` |
| Tempo, meters, and transport | `crates/notwig-app/tests/transport.rs` |
| Latency compensation | `crates/notwig-engine/tests/pdc.rs` |
| Loop wrapping | `crates/notwig-engine/tests/transport.rs` |

UI tests run `egui` headlessly with `Context::default()` and `RawInput`. Use
`notwig_ui::test_api` only from tests; it is `#[doc(hidden)]`.

## Extension recipes

### Add a document command

1. Add the variant to `CommandPayload` in `notwig-core/src/protocol.rs`.
2. Update `requires_plan_rebuild` if the command changes graph topology.
3. Implement application in `notwig-core/src/apply.rs`; emit `EngineMsg` only
   when the engine needs to react.
4. Add a `Workspace` method in `notwig-app/src/workspace/`.
5. Add a `UiIntent` variant and handle it in `gui.rs`.
6. Add tests at the apply layer and, if UI-driven, at the workspace layer.

### Add a stock device

1. Define the spec and processor in `notwig-dsp/src/stock.rs`.
2. Register it in `stock_registry`.
3. Add parameters with sensible ranges/defaults and smoothing.
4. Add UI-specific presentation only if the generic device panel is insufficient.
5. Add DSP rendering tests in `crates/notwig-dsp/tests/stock.rs`.

### Add a keyboard shortcut

1. Add a `ShortcutCommand` variant and its `CommandSpec` entry in
   `notwig-ui/src/commands.rs`, including a stable id and defaults.
2. Handle the command in `notwig-ui/src/views/shell.rs` (or the focused editor) with
   `shortcuts.was_pressed(ctx, ShortcutCommand::YourCommand)`.
3. Emit a `UiIntent` or call the existing workspace/UI behavior.
4. Add the user-facing action to `docs/ROADMAP.md` when it lands.
5. Add a config round-trip or capture test under `crates/notwig-ui/tests/`.

### Add a UI panel or window

1. Keep view code in `notwig-ui`.
2. Emit `UiIntent`; do not mutate `ProjectDoc` directly.
3. Put behavior in `Workspace` or `gui.rs`.
4. Reuse `widgets/`, `timeline/`, and `form.rs` helpers instead of local copies.
5. Add a headless render test in `crates/notwig-ui/tests/`.

### Add an extension device

1. Build the device against `notwig-ext-api` exports (`notwig_device_*`) and
   ship a manifest plus wasm module under the user extensions directory.
2. Return a `DeviceSpec` from `notwig_device_describe`; set `DeviceUiSpec` to
   `Generic` to let the host assign the extension panel renderer.
3. Return a `PanelDraw` from `notwig_device_draw` using draw commands or native
   knob/toggle commands.
4. Keep DSP in `notwig_device_process` with block-based interleaved `f32` audio.
5. Add a WAT fixture in `crates/notwig-ext/tests/` and a headless panel test in
   `crates/notwig-ui/tests/`.

### Add an audio feature

1. Decide whether it belongs to asset import, engine execution, or app audio I/O.
2. Keep file decoding and resampling in `notwig-project`.
3. Keep real-time processing in `notwig-engine`/`notwig-dsp`.
4. Keep device/stream selection and snapshots in `notwig-app/src/audio.rs`.
5. Follow the real-time rules in `AGENTS.md` and `docs/ARCHITECTURE.md`.

## Known structural hotspots

These are the largest or most mixed modules. Split along concrete seams when
changing them; do not add unrelated responsibilities.

- `crates/notwig-ui/src/views/audio_editor/panel.rs` — view, gestures, lowering, and
  clip-specific state are all in one module.
- `crates/notwig-ui/src/views/arranger/mod.rs` — track lanes, clip interaction, and
  timeline gestures are still coupled.
- `crates/notwig-ui/src/views/shell.rs` — top bar, status bar, shortcuts, and windows.
- `crates/notwig-app/src/workspace/mod.rs` — still owns several command groups;
  asset, engine, and plugin concerns have already been extracted.

Deferred structural work:

- Split the clip editor and arranger by view/interaction/lowering.
- Extract an `AudioSettingsDraft` type so driver/device/sample-rate reset policy
  is not spread across UI closures.
- Add a shared popup-window builder for settings, render, DSP, and future dialogs.

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
