# Project Persistence & Lifecycle (`notwig-project`)

## 1. Lifecycle Operations
- `New(meta|template) -> ProjectDoc` (v0 supports the built-in default template only; user templates deferred)
- `Open(bundlePath) -> LoadResult{doc, recoveryOffered?}`
- `Save(doc, bundlePath)` / `SaveAs(doc, newPath)`
- `ImportAsset(path) -> AssetEntry` (decode/resample/ingest into bundle `assets/`)
- `ExportAudio(range, path)` (non-RT Master mixdown; executed by engine/service, coordinated here)
- `Close()`

- Open/New while running: stop transport, build/validate the new document, swap at the control boundary, rebuild the engine plan, and reset runtime state. Failure leaves the previous project active.
- Loaded projects open stopped (`transport.playing=false`) at their last saved locate.

v0 excludes collaboration, branching, recording, undo/redo history persistence, and project sample-rate/tempo/time-signature changes.

## 2. Canonical Bundle Layout
`MyProject.notwig/` directory package:
- `project.json` — manifest + serialized `ProjectDoc` (canonical UTF-8 JSON: sorted keys, no NaN/Inf, fixed number formatting; git-friendly diffs preferred).
- `assets/` — managed imported binary assets named `sha256.<ext>` (recording is excluded in v0).
- `state/` — opaque device/plugin state blobs named `sha256`, referenced by `DeviceInstance.stateRef`; stock device parameter values are stored inline in `project.json`.
- `autosave/` — rolling snapshots for recovery (e.g. `snapshot-<rev>-<ts>.json`).
- `peaks/` — derived waveform/overview cache (regenerable).
- `cache/` — regenerable artifacts (DSP compile caches) optional and omittable from portable copy; user-level plugin scan caches live outside the bundle.

## 3. Manifest & Versioning
`Manifest = { schema: SchemaVersion, appVersion?: String, docPath: "project.json", assets: [AssetEntry], createdUtc, modifiedUtc }` (may embed doc)
- Load: read -> parse -> validate schema -> run migration chain `M_{v -> v+1}` while `major` equal and supported -> semantic validation via Core -> resolve device specs (`typeId/version`) and asset refs. Missing plugins/assets produce explicit issues but do not silently drop state.
- Save: write temp files -> fsync -> atomic rename/replace where possible; update manifest hashes/metadata.
- Hashing: SHA-256 for assets/state blobs; manifest records content hashes for integrity checks.
- Unknown/unsupported `major` => load error (`UnsupportedVersion`). Forward-compat not required in v0.
- Project sample rate, tempo, and time signature are fixed at creation/load; changing them is not supported in v0 (sample-rate changes require re-import/resample and plugin re-prepare; tempo/time-signature changes are reserved for a future `TempoMap`).

## 4. Autosave & Recovery
- Triggers (policy, off-RT): periodic timer and/or N accepted mutations/rev changes; debounce to avoid thrash.
- Writes consistent snapshot of `ProjectDoc` (and required refs) to `autosave/`.
- Saving while playing persists the last locate/last-synced playhead; playback continues and the engine remains authoritative for the live playhead.
- Crash detection: presence of "dirty session" marker/lock not cleared by clean close.
- On open with recovery candidate: present user choice (recover latest autosave vs last clean save). Chosen state must pass same validation as normal load.

## 5. Assets & Relink
`AssetEntry = { id: Id<Asset>, kind: Audio|Midi|Preset|PluginState|Other, uri: String, hash: Option<String>, format: String, sampleRate?: u32, channels?: u16, durationSamples?: u64, originalSampleRate?: u32, originalChannels?: u16 }`
- Document references assets by `Id<Asset>` only. `AssetIndex`/`AssetEntry` schemas live in `notwig-core`; this crate performs IO/ingest and blob storage.
- Audio import (v0): accept WAV/AIFF only. Decode off-RT to `f32`, apply channel policy (mono -> duplicated stereo; stereo -> averaged mono; more than 2 channels rejected as unsupported in v0), resample deterministically to `Meta.sampleRate` with a tested high-quality offline resampler, then store a managed copy `assets/sha256.<ext>` and record original and converted metadata. Import never mutates the source file.
- Other asset kinds (MIDI, preset, plugin state) are managed references; MIDI-file import and preset browsing are not in v0.
- Missing/unreadable asset: load produces explicit `MissingAsset` issue and relink flow (update `uri`/rebind entry) rather than silent removal. Affected clips play silence and stay flagged until relink validation succeeds.
- `SaveAs` copies referenced managed assets/state blobs into the new bundle; unreferenced assets are not garbage-collected automatically in v0 (explicit cleanup only).

## 6. Export (v0)
- `ExportAudio(range, path)` renders the Master output offline and writes WAV at `Meta.sampleRate`, 32-bit float PCM; no stems, normalization, dither, or compressed export in v0.
- Export uses the same semantic graph/scheduling as playback; deterministic stock DSP is bit-comparable within `ε`, while hosted plugins may be non-deterministic (see `../notwig-dsp/REALTIME_OFFLINE.md`).

## 7. Global vs Project Settings
- The project bundle contains only document/assets/state/caches. User-level settings (audio/MIDI devices, preferred buffer/rate, plugin scan paths, theme, recent projects, logging) live in the platform config/cache dirs managed by `notwig-app`.
- v0 does not support project-level audio-device overrides or sample-rate changes.
