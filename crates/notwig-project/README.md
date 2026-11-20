# notwig-project

Project bundle persistence and audio asset I/O.

## Purpose

`notwig-project` turns a `ProjectDoc` into a durable `*.notwig` bundle and loads
it back. It also owns audio file decoding, offline resampling, asset import, and
audio export. It deliberately contains no UI, no engine execution, and no
real-time code.

## Owns

- Saving and opening the project bundle directory.
- Atomic JSON writes for the project manifest and autosave snapshots.
- The versioned JSON manifest and schema-major checks.
- Autosave snapshots, cleanup, and recovery detection.
- `session.lock` creation/removal helpers used by the app to detect unclean exits.
- Header-only audio probing for duration, sample rate, and channels during import previews.
- WAV/AIFF/MP3/FLAC/OGG decoding through `symphonia`.
- Offline resampling to the project sample rate with `rubato`.
- Managed content-addressed asset copies under `assets/`, hashed from the encoded
  WAV bytes before they are written.
- WAV/AIFF/FLAC/MP3/OGG writing with bit depth where applicable, optional dithering and normalization.
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
  session.lock    # present only while a session owns the bundle
```

`project.json` contains a `Manifest` with schema, app version, asset entries,
timestamps, and the full `ProjectDoc`. The embedded `doc` is authoritative; the
top-level manifest copies are advisory and their schema is checked against
`doc.schema` on load. Manifest saves, autosaves, and asset finalization use
temp-file-and-rename writes.

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
| `assets.rs` | Decode, import, hash, and persist audio assets |
| `probe.rs` | Header-only duration, sample-rate, and channel probing |
| `resample.rs` | `rubato`-based offline sample-rate conversion |
| `export.rs` | WAV/AIFF/FLAC/MP3/OGG export, bit depth, dithering, normalization |
| `fsutil.rs` | Atomic file/JSON writes and directory creation |
| `error.rs` | `ProjectError` |

## Tests

Integration tests in `tests/` cover bundle round-trips, autosave/recovery,
session lock lifecycle, corrupt autosaves, asset import, resampling, and export.
Audio fixtures live in `tests/fixtures/`.
