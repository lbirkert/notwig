# notwig-ui

Reusable `egui` views, interaction, and runtime projections.

## Purpose

`notwig-ui` renders the application and translates user gestures into
`UiIntent` values. It owns view state but never canonical project state. It does
not depend on engine internals; it reads a `RuntimeView` prepared by the app.

## Owns

- The `egui` shell: top bar, transport strip, status bar, shortcuts, and windows.
- Arranger view and timeline interactions.
- Audio clip editor and device editor bottom panels.
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
| `shell.rs` | App shell, top/status bars, layout, shortcuts |
| `arranger.rs` | Track lanes and timeline clip interaction |
| `timeline/` | Beat strip, clip visuals, waveform, grid, selection, drop preview, interaction math |
| `panel/` | Shared panel chrome, device editor, audio clip editor |
| `info/` | Left info panel sections |
| `device/` | Device browser, parameter views, EQ |
| `settings.rs`, `render.rs`, `dsp.rs` | Popup windows |
| `widgets/` | Knob, fader, draggable number, formatting, snapping |
| `state.rs`, `intent.rs`, `runtime.rs` | View state, intents, runtime projection |
| `theme.rs`, `icons.rs` | Visual tokens and icons |

## Tests

Headless `egui` tests live in `tests/`. They cover rendering, selection,
shortcuts, gestures, timeline math, widgets, and headless window layouts. Use
`notwig_ui::test_api` only from tests.

See [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) for the UI's place in
the data flow.
