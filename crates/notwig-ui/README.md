# notwig-ui

Reusable `egui` views, interaction, extension components, and runtime
projections.

## Purpose

`notwig-ui` renders the application and translates user gestures into
`UiIntent` values. It owns view state but never canonical project state. It does
not depend on engine internals; it reads a `RuntimeView` prepared by the app.

The `components` and `palette` modules are dependency-light and also used by
extension devices inside their guest egui context. Disable default features
(`default-features = false`) to compile only those modules without the app shell.

## Owns

- The `egui` shell: app bar, transport toolbar, status bar, shortcuts, and windows.
- Arranger view and timeline interactions.
- Audio clip editor, MIDI editor, and device editor bottom panels.
- Info panel for tracks, devices, clips, files, and time selections.
- Device browser and parameter/EQ views.
- Settings, render mixdown, and DSP load windows.
- Reusable widgets, timeline primitives, theme, icons, and formatting helpers.
- Shared extension-facing components: knobs, faders, stereo meters, labels,
  sections, toggles, dropdowns, and parameter formatting.
- `UiIntent` and drag/drop target types.
- `RuntimeView`, meters, audio settings, and DSP analysis view types.
- Pure view state: selection, viewport, open editor, notifications.
- Guest egui panel compositing: per-panel texture namespaces, mesh painting,
  input/focus bridging, and validated action application.

## Does not own

- `ProjectDoc` mutation or command application (see `notwig-app::Workspace`).
- Audio device I/O or engine lifecycle (see `notwig-app`).
- Domain types or command semantics (see `notwig-core`).
- DSP compilation or device processing (see `notwig-core::dsp`/`notwig-engine`).

## Module map

| Module | Contents |
|---|---|
| `components/` | Extension-facing egui controls, meters, and formatters |
| `palette.rs` | Shared color constants used by the app and guest components |
| `views/shell/` | App shell, app bar, transport/status bars, layout, shortcuts, overlays, help, drag/drop |
| `views/arranger/` | Track lanes, clip interaction, navigation, drop preview, edit helpers; track split by header/lane/drop/menu |
| `views/dsp.rs` | DSP load analysis window |
| `views/info/` | Left info panel sections |
| `views/panel.rs` | Shared bottom-panel chrome and layout helpers |
| `views/audio_editor/` | Audio clip editor: state, actions, geometry, model, navigation, gain line-editor adapter; panel split into range selection, slice paint, slice gestures |
| `views/midi_editor/` | MIDI note editor: state, model, geometry, actions, navigation; panel split into paint, note drag, velocity lane, audition |
| `views/device_editor/` | Device panel, browser, parameters, EQ, dispatch; card/stock body/knob/bracket/plugin-kind split |
| `views/settings/` | Settings window/navigation plus audio, audio channels, extensions, and shortcuts |
| `commands/` | Shortcut command catalog, chords, bindings, and config |
| `views/render.rs` | Render mixdown window |
| `widgets/` | App-only knobs, faders, draggable numbers, form helpers, brand assets, icons, timeline |
| `widgets/timeline/` | Beat strip (`beat_strip/` interaction + paint), clip visuals, grid, selection, drop preview, interaction math |
| `widgets/line_editor.rs` | Reusable point/segment line editor for automation, clip gain, pan, and other modulatable values |
| `widgets/area.rs` | Shared single-mesh filled-area rendering for curves and waveforms |
| `fine_drag.rs` | Shared coarse/fine drag offsets that preserve coarse motion when Shift toggles mid-drag |
| `waveform.rs` | Signed min/max waveform pyramid, level-of-detail selection, and sample-level drawing |
| `widgets/drop_target.rs` | Drag preview and drop-target resolution |
| `clipboard.rs` | Shared arranger/audio-clip clipboard payloads |
| `state/`, `intent.rs`, `runtime.rs` | View state (selection/viewport/notifications), intents, runtime projection |

## Tests

Headless `egui` tests live in `tests/`. They cover rendering, selection,
shortcuts, gestures, timeline math, widgets, extension panel compositing, MIDI
editor editing, and headless window layouts. Use `notwig_ui::test_api` only from
tests.

See [`docs/03_development/00_architecture.md`](../../docs/03_development/00_architecture.md) for the UI's place in
the data flow.
