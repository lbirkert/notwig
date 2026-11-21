# Projects and Recovery

## Project bundles

A project is a directory ending in `.notwig`. The directory keeps the document,
audio assets, autosaves, and derived data together:

```text
my-project.notwig/
  project.json       manifest and document
  assets/            imported audio, stored as content-addressed float WAV
  autosave/          timed snapshots while the project is dirty
  state/             persisted plugin state blobs
  peaks/             reserved for the waveform cache
  cache/             reserved for derived data
```

Use **FILE > Save** to write the current bundle and **Save As…** to copy it to a
new location. Saving writes through a temporary file and renames it into place,
so an interrupted save does not leave a half-written `project.json`.

Notwig also accepts a project folder that contains the bundle layout. Use
**Open Project Folder…** when the directory does not end in `.notwig`.

## Importing audio

**FILE > Import Audio…** imports a file into `assets/`. You can also drag one or
more audio files from Finder directly onto an audio track, the arranger, or the
audio clip editor.

- WAV, AIFF, MP3, FLAC, and OGG are the supported import paths today.
- Imported audio is decoded and resampled to the project sample rate, then
  stored as float WAV inside the bundle.
- Dragging several files onto a track places the first file at the drop point
  and creates new tracks for the rest.
- If an asset is missing when a project opens, the loader reports a load issue.
  Relinking currently stores the new path and decodes the file; it does not copy
  the file into the bundle or verify its hash.

Audio assets are referenced by ID, so moving a project directory keeps its
audio together.

## Autosave and the session lock

Notwig autosaves while the project is dirty, at most once per minute. Each
snapshot is written under `autosave/` with the revision and a timestamp.

When a native project is opened or saved, Notwig creates a `session.lock` file
in the bundle. A clean exit removes the lock. If Notwig exits without removing
the lock and autosaves exist, the next open offers recovery.

Notwig also remembers recently opened and saved bundles in a per-user registry.
When the application starts without a project argument, it checks those bundles
for a stale `session.lock` and autosave snapshots, then offers every recovery
candidate in the same modal. Select the project you want before choosing an
action.

The recovery modal has two actions:

- **Recover latest autosave** loads the newest snapshot and marks the project
  dirty. The history starts fresh.
- **Discard** removes the autosave snapshots and the stale session lock.

Recovery is a project-level feature. Shortcut bindings and audio device
settings are stored separately in the per-user configuration directory:

```text
~/Library/Application Support/notwig/
  shortcuts.json
  audio.json
  projects.json
  extensions.json
  extensions/
```

On other platforms Notwig uses the equivalent `.config/notwig` path where
implemented. The project sample rate is part of the project, not the user
configuration.

## Dirty state and unsaved work

The window title shows a dirty marker while the document has unsaved changes.
Opening or creating another project does not currently prompt about unsaved
work. Save before switching projects if you need the current state.

## What is not in a project

- Native plugin binaries are not copied into the bundle. A project stores the
  plugin identity and state, so the plugin must be installed to load it.
- Extension packages live in the user extension directory, not in the project.
- Window layout, selections, and viewport positions are session state.
- Render output is written wherever you choose; it is not part of the bundle.

Next: [Tracks and Mixing](03_tracks_and_mixing.md).
