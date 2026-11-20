# notwig-app

Application composition, runtime wiring, and the `notwig` binary.

## Purpose

`notwig-app` is the top layer. It opens projects, owns the workspace and undo
history, starts the audio stream, builds engines off-thread, handles dialogs and
file drops, and connects `notwig-ui` intents to `notwig-core` commands. Reusable
domain or DSP logic belongs in lower crates, not here.

## Owns

- CLI entrypoint and commands (`ui`, `new`, `demo`, `render`, `info`,
  `scan-plugins`).
- The `eframe` app loop and UI intent handling for native and browser targets.
- Browser entry composition, OPFS/File System Access storage, async file
  pickers, and user-gesture-gated WebAudio startup.
- `Workspace`: document mutation, command application, undo/redo, tracks,
  devices, transport, and project lifecycle policy.
- Asset import/relink/preparation for the engine.
- Session lock lifecycle and the startup autosave recovery flow.
- Async engine build queue, poll, install, and transport preservation.
- CPAL audio stream, device options, snapshots, DSP timing, and xruns.
- Per-user persistence of audio driver, device, and block size.
- Plugin catalog: scan subprocess, cache loading, and workspace descriptors.
- Edit-plan lowering into commands.
- Native plugin editor windows.
- Dialog and drag-drop coordination.

## Does not own

- Domain types, validation, or command semantics (see `notwig-core`).
- Bundle format or audio decoding (see `notwig-project`).
- Device contracts or graph compilation (see `notwig-dsp`).
- Real-time processing (see `notwig-engine`).
- Plugin format adapters (see `notwig-plugins`).
- Reusable views or widgets (see `notwig-ui`).

## Module map

| Module | Contents |
|---|---|
| `main.rs` | CLI, demo project, offline render/info commands |
| `gui.rs` | `eframe` app, intent handling, native dialogs, plugin windows |
| `web/mod.rs` | Browser app construction |
| `web/storage.rs` | OPFS and File System Access project storage |
| `clock.rs` | Native/browser clock selection |
| `workspace/mod.rs` | Project lifecycle, commands, history, tracks/devices/transport |
| `workspace/assets.rs` | Asset load/import/relink/peaks/engine prepare |
| `workspace/recovery.rs` | Session lock lifecycle, autosave recovery, discard |
| `engine_build.rs` | Async engine build lifecycle |
| `audio.rs` | CPAL stream, options, snapshot, DSP timing |
| `audio_config.rs` | Persisted audio driver/device/block-size config |
| `plugins.rs` | Plugin catalog/scanner/cache glue |
| `edit_lowering.rs` | Edit plan to command lowering |
| `history.rs` | Undo/redo stack |
| `logging.rs` | Tracing setup |

## Tests

Integration tests in `tests/` cover workspace commands, undo/redo, async engine
builds, sample-rate changes, clip operations, import/render flows, MIDI note
editing, audio config persistence, and session recovery.
