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
- Browser entry composition, OPFS `Projects` folder storage, async file
  pickers, ZIP download, and user-gesture-gated WebAudio startup.
- Browser extension index fetching, OPFS extraction/discovery, uninstall, and
  WASM extension loading.
- `Workspace`: document mutation, command application, undo/redo, tracks,
  devices, transport, and project lifecycle policy.
- Asset import/relink/preparation for the engine.
- Session lock lifecycle and the startup autosave recovery flow.
- Native engine child process (`engine-host`), IPC protocol, snapshots,
  plugin editor hosting, and crash/restart handling.
- Async engine build queue, poll, install, and transport preservation.
- CPAL audio stream, device options, snapshots, DSP timing, and xruns.
- Per-user persistence of audio driver, device, block size, named input/output
  channels, and master output.
- Browser microphone capture through Web Audio with the same input-bus and
  recording path as native CPAL input.
- Plugin catalog: scan subprocess, cache loading, crash-isolated metadata probe,
  and workspace descriptors.
- Extension manager: config/index sources, target selection, hash-verified ZIP
  install/uninstall, host lifecycle, and app synchronization.
- Edit-plan lowering into commands.
- Native plugin editor windows.
- Dialog and drag-drop coordination.

## Features

| Feature | Meaning |
| --- | --- |
| `native` | Native binary and platform integration |
| `au` | AU v2 and AU v3 hosting |
| `clap` | CLAP hosting |
| `vst` | VST3 and VST2 hosting |
| `lv2` | LV2 hosting; requires system `lilv` |
| `web` | Browser target support |

The default native build enables `au`, `clap`, and `vst`, but not `lv2`.
Integration tests can run without native plugin formats:

```sh
cargo test -p notwig-app --no-default-features --features native
```

## Does not own

- Domain types, validation, or command semantics (see `notwig-core`).
- Bundle format or audio decoding (see `notwig-project`).
- Device contracts or graph compilation (see `notwig-core::dsp`).
- Real-time processing (see `notwig-engine`).
- Plugin format adapters (see `notwig-plugins`).
- Reusable views or widgets (see `notwig-ui`).

## Module map

| Module | Contents |
|---|---|
| `lib.rs` | Shared native/browser library surface |
| `main.rs` | Clap-based CLI, demo project, offline render/info commands |
| `gui/` | `eframe` app facade plus intents, audio supervision, recording, render, dialogs, plugin windows, extensions, platform |
| `web/mod.rs` | Browser app construction |
| `web/storage.rs` | OPFS project and extension storage, project picker, and ZIP download |
| `clock.rs` | Native/browser clock selection |
| `workspace/` | Lifecycle facade plus commands, engine, persistence, transport, tracks, devices, clips, sample rate, render |
| `workspace/assets.rs` | Asset load/import/relink/peaks/engine prepare |
| `workspace/recovery.rs` | Session lock lifecycle, autosave recovery, discard |
| `engine_build.rs` | Async engine build lifecycle |
| `audio/` | Audio settings and device enumeration, snapshots and DSP timing, CPAL stream builders, sample-format dispatch, and `AudioService` wiring |
| `audio_worker.rs` | Render-ahead project/monitor worker and ring handoff |
| `loopback.rs` | Native loopback latency and clock-drift measurement |
| `audio_config.rs` | Persisted audio driver/device/block-size, named-channel, and master-output config |
| `app_paths.rs` | Canonical per-user app, cache, and log directories |
| `audio_input/mod.rs` | Shared input requests and platform input-hub selection |
| `audio_input/native.rs` | CPAL input streams and hardware input enumeration |
| `audio_input/web.rs` | Web Audio microphone capture and browser input enumeration |
| `plugins/mod.rs` | Target-selected plugin catalog exports |
| `plugins/native.rs` | Plugin catalog, scan subprocess, and cache glue |
| `plugins/web.rs` | Empty browser plugin catalog |
| `extensions/mod.rs` | Target-selected extension manager and events |
| `extensions/index.rs` | Shared index URL resolution, validation, and version deduplication |
| `extensions/native.rs` | Extension config/index fetch, install/uninstall, wasmtime host lifecycle |
| `extensions/web.rs` | Index fetch, OPFS install/discovery, browser host lifecycle, and app sync |
| `shortcuts.rs` | User shortcut config load/save |
| `edit_lowering.rs` | Edit plan to command lowering |
| `history.rs` | Undo/redo stack |
| `logging.rs` | Tracing setup, timestamped app/engine files, and OPFS app logs |

## Tests

Integration tests in `tests/` cover workspace commands, undo/redo, async engine
builds, sample-rate changes, clip operations, import/render flows, MIDI note
editing, audio config persistence, and session recovery.
