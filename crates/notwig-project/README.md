# notwig-project

Project bundle persistence and audio asset I/O.

## Purpose

`notwig-project` turns a `ProjectDoc` into a durable `*.notwig` bundle and loads
it back. It also owns audio file decoding, offline resampling, asset import, and
WAV export. It deliberately contains no UI, no engine execution, and no
real-time code.

## Owns

- Saving and opening the project bundle directory.
- The versioned JSON manifest and schema-major checks.
- Autosave snapshots, cleanup, and recovery detection.
- WAV/AIFF/MP3/FLAC/OGG decoding through `symphonia`.
- Offline resampling to the project sample rate.
- Managed asset copies under `assets/`.
- 32-bit float WAV writing.
- `ProjectError` and load-issue reporting.

## Bundle layout

```text
project.notwig/
  project.json
  assets/
  peaks/
  autosave/
  state/
  cache/
```

`project.json` contains a `Manifest` with schema, app version, asset entries,
timestamps, and the full `ProjectDoc`.

## Does not own

- UI, dialogs, or user-facing prompts.
- Audio callback execution.
- Domain validation rules (those live in `notwig-core`).
- Plugin discovery or hosting.

## Module map

| Module | Contents |
|---|---|
| `bundle.rs` | Save/open/autosave/recovery |
| `manifest.rs` | Manifest, schema version, load issues |
| `assets.rs` | Decode, resample, import, WAV export |
| `error.rs` | `ProjectError` |

## Tests

Integration tests in `tests/` cover bundle round-trips, autosave/recovery,
asset import, resampling, and export. Audio fixtures live in `tests/fixtures/`.
