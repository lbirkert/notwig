# Code structure

This page maps the repository and describes where different kinds of changes
belong. It is intentionally practical: use it to find the right crate, module,
and test layer before editing.

## Repository layout

```text
.
  Cargo.toml                 workspace, dependencies, lint policy
  README.md                  project overview and build instructions
  AGENTS.md                  contributor/agent workflow and code style
  docs/                      authoritative documentation
  plan/                      historical planning archive (not authoritative)
  crates/
    notwig-core/
    notwig-project/
    notwig-dsp/
    notwig-engine/
    notwig-plugins/
    notwig-ui/
    notwig-app/
```

Every crate has a `README.md` describing its purpose and boundaries. The
workspace enforces `#![forbid(unsafe_code)]` and a strict Clippy policy.

## Crate dependency graph

```text
notwig-app -> notwig-ui, notwig-core, notwig-project,
              notwig-engine, notwig-plugins, notwig-dsp
notwig-ui -> notwig-core, notwig-project, notwig-dsp
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
| `adapters.rs` | VST3/AU adapters built on `truce-rack` |
| `cache.rs` | Persisted plugin scan cache |
| `types.rs` | Format-agnostic descriptors, states, capabilities, errors |
| `native_editor.rs` | Native plugin editor window lifecycle |
| `rack.rs` | `truce-rack` integration details |

### `notwig-ui` — views and interaction

| Module | Responsibility |
|---|---|
| `shell.rs` | Top bar, transport, status bar, layout, shortcuts, notifications |
| `arranger.rs` | Track lanes, timeline clips, drag/move/trim, selection gestures |
| `timeline/` | Shared beat strip, clip visuals, waveform, grid, selection, drop preview, interaction math |
| `panel/` | Bottom panels: shared chrome, device editor, audio clip editor |
| `info/` | Left info panel: track, device, MIDI clip, audio clip, files, time |
| `device/` | Device browser, parameter widgets, EQ display |
| `settings.rs` | Settings window; currently the Audio page is implemented |
| `render.rs` | Render mixdown window |
| `dsp.rs` | DSP load analysis window |
| `widgets/` | Reusable knob, fader, draggable number, formatting, snapping |
| `state.rs` | Selection, viewport, open editor, notifications, edit focus |
| `intent.rs` | `UiIntent` and drag/drop target types |
| `runtime.rs` | Read-only runtime projection (`RuntimeView`, meters, audio/DSP views) |
| `theme.rs` / `icons.rs` | Visual tokens and icon helpers |

### `notwig-app` — composition and runtime policy

| Module | Responsibility |
|---|---|
| `main.rs` | CLI entrypoint, demo/new/render/info/scan commands |
| `gui.rs` | `eframe` app, intent handling, dialogs, plugin windows, runtime projection |
| `workspace/mod.rs` | Project lifecycle, command application, undo/redo, tracks, devices, transport |
| `workspace/assets.rs` | Asset loading, import, relink, peaks, engine asset preparation |
| `engine_build.rs` | Async engine build request/queue/poll lifecycle |
| `audio.rs` | CPAL audio service, device options, snapshot, DSP timing |
| `plugins.rs` | Plugin catalog/cache/scanner subprocess glue |
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
| Engine playback/rendering | `crates/notwig-engine/tests/` |
| Plugin cache/types | `crates/notwig-plugins/tests/` |
| UI headless rendering, gestures, state | `crates/notwig-ui/tests/` |
| Workspace and history integration | `crates/notwig-app/tests/` |

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

### Add a UI panel or window

1. Keep view code in `notwig-ui`.
2. Emit `UiIntent`; do not mutate `ProjectDoc` directly.
3. Put behavior in `Workspace` or `gui.rs`.
4. Reuse `widgets/`, `timeline/`, and `form.rs` helpers instead of local copies.
5. Add a headless render test in `crates/notwig-ui/tests/`.

### Add an audio feature

1. Decide whether it belongs to asset import, engine execution, or app audio I/O.
2. Keep file decoding and resampling in `notwig-project`.
3. Keep real-time processing in `notwig-engine`/`notwig-dsp`.
4. Keep device/stream selection and snapshots in `notwig-app/src/audio.rs`.
5. Follow the real-time rules in `AGENTS.md` and `docs/ARCHITECTURE.md`.

## Known structural hotspots

These are the largest or most mixed modules. Split along concrete seams when
changing them; do not add unrelated responsibilities.

- `crates/notwig-ui/src/panel/clip_editor.rs` — view, gestures, lowering, and
  clip-specific state are all in one module.
- `crates/notwig-ui/src/arranger.rs` — track lanes, clip interaction, and
  timeline gestures are still coupled.
- `crates/notwig-ui/src/shell.rs` — top bar, status bar, shortcuts, and windows.
- `crates/notwig-app/src/workspace/mod.rs` — still owns several command groups;
  asset, engine, and plugin concerns have already been extracted.

Deferred structural work:

- Split the clip editor and arranger by view/interaction/lowering.
- Extract an `AudioSettingsDraft` type so driver/device/sample-rate reset policy
  is not spread across UI closures.
- Add a shared popup-window builder for settings, render, DSP, and future dialogs.
- Continue the audio hardening items in the historical `plan/AUDIO.md` notes:
  plugin load timeouts, import worker, streaming export, and stock DSP cleanup.

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
