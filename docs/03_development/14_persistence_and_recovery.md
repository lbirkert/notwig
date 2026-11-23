# Persistence and recovery

Status: native bundles, autosave, and session recovery are implemented. Browser
builds reuse `BundleFiles` but have no autosave, lock, or recovery. Related:
[Architecture](00_architecture.md), [Code Structure](01_code_structure.md),
[Projects and Recovery](../01_users/02_projects_and_recovery.md), and
[Web build and storage](20_web_build_and_storage.md).

## Bundle layout

A project is a `.notwig` directory. Saving creates every directory below even
when it is unused:

```text
song.notwig/
  project.json    Manifest wrapper; its `doc` field is authoritative
  assets/         float WAV audio, content-addressed or ID-named
  autosave/       snapshot-<revision>-<unix-seconds>.json
  state/          <device-id>.json plugin state blobs
  peaks/          reserved for waveform caches
  cache/          reserved for derived data
  session.lock    present while a native session owns the bundle
```

`notwig-project::save` only ensures the directories and writes `project.json`;
the app writes plugin state and pending imported assets first.

## Manifest authority and schema checks

`project.json` is a `Manifest` wrapper. Its top-level `schema`, `assets`,
`created_utc`, and `modified_utc` fields are advisory copies for external
inspection; loading reads project state only from `doc`. `Manifest::new` sets
`doc_path` to `project.json` and records the crate version in `app_version`.

A wrapped manifest is accepted when `manifest.schema == doc.schema` and both
majors match; the loader checks each major (`UnsupportedVersion`) and then the
equality (`ProjectError::Schema`). A raw document with no `doc` key is the legacy path and is
accepted when `doc.schema.major` matches `SCHEMA_VERSION.major`. Only majors are
compared, minor versions are ignored, and no migration runs; `SCHEMA_VERSION` is
currently `V0` with major `0`.

After parsing, `normalize_loaded` clears `transport.playing`,
`transport.recording`, and every `track.record_armed`. Failed validation and
missing asset files become `ManifestIssue` entries but do not block opening.

## Atomic writes and session lock

`fsutil::write_atomic` writes `target.tmp`, creates parent directories, then
renames onto the target. The fixed temporary name (target plus `.tmp`) lets
concurrent saves to one path collide. There is no `fsync`; the rename leaves
either the old file or the new file, not a partial one.

The lock is a plain file, not exclusive. `create_session_lock` writes
`pid=<pid>`; existence is the only signal. The app creates it in `begin_session`
after opening or saving, removes it in `end_session` on clean exit, and removes
the previous bundle's lock when Save As changes the path. Create/remove failures
are logged (creation also sets status) but do not abort.

| Action | Reads | Writes | Lock | Recovery offered |
|---|---|---|---|---|
| Open bundle | `project.json`; assets on workspace load | none | create | lock and autosaves exist |
| Save | current document and in-memory state | `state/`, pending `assets/`, `project.json` | create (remove old path) | unaffected |
| Autosave | document and revision | one `autosave/*.json` | unchanged | unchanged |
| Recover | newest autosave JSON, then assets | replaces in-memory document | unchanged | cleared |
| Discard | none | deletes autosaves and lock | removed; next session recreates | cleared |

## Audio assets and integrity

Imported audio is decoded with Symphonia (WAV, AIFF, MP3, FLAC, OGG), rejected
above two channels, resampled to the project rate, and mono is duplicated to
stereo. `import_asset` encodes float WAV, hashes those bytes with SHA-256, and
renames `assets/<asset-id>.wav.tmp` to `assets/<hash>.wav`; the entry records
`uri`, `hash`, rates, channels, and duration. Recordings use
`assets/<asset-id>-<rate>.wav` and browser saves use `assets/<asset-id>.wav`,
both with `hash = None`. Loading only checks that the `uri` file exists and
decodes it; it never verifies the hash. Relinking stores a new path without
copying or hashing.

## Plugin state blobs

Native saves serialize each `PluginState` to `state/<device-id>.json` and set
`device.state_ref = { hash: "<device-id>", encoding: "notwig.plugin-state.json" }`.
Loading reads only that encoding and skips missing or unparseable blobs. The blob
write is a direct `std::fs::write`, not the atomic helper. Autosave stores only
the document, so its state refs can be newer than the blobs from the last save.
The browser `bundle_files` writes the same blobs, but the wasm state loader is a
no-op and `read_bundle` reads only `project.json` plus asset URIs.

## Autosave

Autosave is native-only and runs only while dirty. It is limited to one attempt
per 60 seconds, is skipped while an asynchronous audio import is pending, and
writes `autosave/snapshot-<revision>-<unix-seconds>.json` with
`write_json_atomic`. There is no pruning; snapshots accumulate until discard.
A failed write is logged and surfaced in the workspace status, and the autosave
deadline is not advanced, so the next frame can retry once the interval elapses.

`list_autosaves` keeps `.json` files and sorts by the parsed revision and
timestamp, then by path for ties; the unpadded names are never compared as text.
`load_latest_autosave` pops the last path. A corrupt newest snapshot returns an
error instead of falling back.

## Recovery flow

Recovery is offered when a bundle has both `session.lock` and at least one
autosave. Startup with no project argument scans the per-user project registry
for candidates; opening a bundle checks the same condition.

```text
startup / open bundle
   +-- no lock or no autosave --> normal load
   +-- lock + autosave --------> modal
          Recover: load newest snapshot, normalize, mark dirty,
                   reset history, reload assets, rebuild engine;
                   keep snapshots and lock
          Discard: delete autosave/*.json and session.lock;
                   the next session recreates the lock
```

`recover_latest_autosave` replaces `doc`, marks it dirty, resets undo history,
clears the playhead anchor, reloads assets and `state/` plugin blobs, and
requests an engine rebuild. It does not restore selection or delete snapshots.
Recover and discard failures leave the lock and snapshots in place so the
candidate stays recoverable.

## Browser byte I/O differences

The browser does not call the filesystem bundle functions. It loads and stores a
`BundleFiles` map of relative path to bytes with `open_files`/`save_files`, then
writes each path through OPFS. Consequences: no temporary-file rename, lock,
autosave, recovery, or cross-file transaction. `read_bundle` requires the wrapped
manifest and reads only asset URIs, not `state/`; Download reads every file
recursively and encodes an uncompressed ZIP.

## Known gaps

- Snapshots are never pruned or removed after recovery.
- Save writes state and assets before `project.json` without rollback and leaves obsolete files behind.
- Hashes are never verified; browser saves and recordings are ID-named with no hash, and browser loading omits `state/` blobs.
- Browser Save As stages a complete bundle before committing, but does not remove stale files left by an older revision.
