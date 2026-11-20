# notwig-ui

Reusable `egui` views, interaction, and runtime projections.

## Purpose

`notwig-ui` renders the application and translates user gestures into
`UiIntent` values. It owns view state but never canonical project state. It does
not depend on engine internals; it reads a `RuntimeView` prepared by the app.

## Owns

- The `egui` shell: top bar, transport strip, status bar, shortcuts, and windows.
- Arranger view and timeline interactions.
- Audio clip editor, MIDI MIDI editor, and device editor bottom panels.
- Info panel for tracks, devices, clips, files, and time selections.
- Device browser and parameter/EQ views.
- Settings, render mixdown, and DSP load windows.
- Reusable widgets, timeline primitives, theme, icons, and formatting helpers.
- `UiIntent` and drag/drop target types.
- `RuntimeView`, meters, audio settings, and DSP analysis view types.
- Pure view state: selection, viewport, open editor, notifications.

## Does not own

- `ProjectDoc` mutation or command application (see `notwig-app::Workspace`).
- Audio device I/O or engine lifecycle (see `notwig-app`).
- Domain types or command semantics (see `notwig-core`).
- DSP compilation or device processing (see `notwig-dsp`/`notwig-engine`).

## Module map

| Module | Contents |
|---|---|
| `views/shell.rs` | App shell, top/status bars, layout, shortcuts |
| `views/arranger/` | Track lanes, clip interaction, track headers, navigation, drop preview, edit helpers |
| `views/dsp.rs` | DSP load analysis window |
| `views/info/` | Left info panel sections |
| `views/panel.rs` | Shared bottom-panel chrome and layout helpers |
| `views/audio_editor/` | Audio clip editor: state, panel, actions, geometry, model, navigation |
| `views/midi_editor/` | MIDI note editor: state, model, geometry, actions, navigation, panel |
| `views/device_editor/` | Device panel, browser, parameter views, EQ |
| `views/settings/mod.rs` | Settings window and category navigation |
| `views/settings/shortcuts.rs` | Shortcut editor page and key capture |
| `commands.rs` | Shortcut command catalog, chords, bindings, and config |
| `views/render.rs` | Render mixdown window |
| `widgets/` | Knobs, faders, draggable numbers, form helpers, icons, theme |
| `widgets/timeline/` | Beat strip, clip visuals, waveform, grid, selection, drop preview, interaction math |
| `widgets/drop_target.rs` | Drag preview and drop-target resolution |
| `clipboard.rs` | Shared arranger/audio-clip clipboard payloads |
| `state.rs`, `intent.rs`, `runtime.rs` | View state, intents, runtime projection |

## Tests

Headless `egui` tests live in `tests/`. They cover rendering, selection,
shortcuts, gestures, timeline math, widgets, MIDI editor editing, and headless
window layouts. Use `notwig_ui::test_api` only from tests.

See [`docs/03_development/00_architecture.md`](../../docs/03_development/00_architecture.md) for the UI's place in
the data flow.
