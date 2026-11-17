# Clip Contracts

Current implementation: `crates/notwig-core/src/sequencer.rs`, `apply.rs`; `crates/notwig-app/src/workspace.rs`; `crates/notwig-ui/src/{edit,clip_editor}.rs`.

Related: [`EDITING.md`](./EDITING.md) for time math and audio-slice editing, [`ARRANGER.md`](./ARRANGER.md) for clip gestures.

## 1. Clip identity

A clip is a `ClipEntity`:

```text
ClipEntity {
    id: ClipId,
    track_id: TrackId,
    kind: Midi | Audio,
    range: [start, end),
    name: Option<String>,
    color: Option<Color>,
    muted: bool,
    payload: Midi(MidiClipData) | Audio(AudioClipData),
}
```

- `ClipId` is the stable identity.
- `range` is non-empty and lies at or after zero.
- `kind` must match the payload variant.
- MIDI clips live only on Instrument tracks; audio clips live only on Audio tracks. Group, BusFx, and Master tracks have no lanes.
- Clip name and colour are part of the model. The current inspector exposes mute/duplicate/delete, not clip rename or recolour.

## 2. Payloads

### MIDI

```text
MidiClipData {
    length: TimeMus,
    content_start: TimeMus,
    notes: Vec<NoteEntity>,
}
```

- Notes have stable `NoteId`s, pitch 0-127, non-negative start, positive length, velocity, optional release velocity, and a reserved expression map.
- `content_start` is the window offset into the content. The visible/played window is `[content_start, content_start + range.length)` clipped to the content.
- Splitting a MIDI clip splits crossing notes at the split; the right side receives new `NoteId`s supplied by the caller.
- There is no MIDI note editor in the current UI. New MIDI clips are empty; notes render only as an arranger preview.

### Audio

```text
AudioClipData {
    asset_id: AssetId,
    source_start_sample: u64,
    source_end_sample: u64,
    gain_db: Db,
    fade_in_samples: u64,
    fade_out_samples: u64,
    fade_shape: Linear,
    slices: Vec<AudioSlice>,
    slices_enabled: bool,
}
```

- Audio assets are shared project resources referenced by `AssetId`; clip edits never modify the asset.
- Each audio slice may carry `asset_id: Option<AssetId>`. `None` means the clip's base asset; `Some` selects another project asset.
- In legacy single-source mode, the clip range length at project tempo equals the source duration.
- Slice mode is active when `slices_enabled` is true or the slice set is non-empty. An explicit set of slices enters slice mode even when the set is empty, which renders silence.
- Audio has non-destructive trim, gain, and linear fades only. There is no time-stretch or warping.
- Slice editing is in [`EDITING.md`](./EDITING.md).

## 3. Creation

- Empty MIDI clip: 4 beats, notes empty (`workspace::create_midi_clip`).
- Audio clip insertion: creates a clip spanning the full asset at the current playhead (`workspace::insert_audio_clip_at`). Dropped files use the drop beat.
- Imported audio is decoded and resampled to the project rate before registration.
- All clip creation carries caller-generated `ClipId`s; the core never allocates identity.

## 4. Overlap policy

After a create, move, duplicate, or range edit, clips on the same lane must not overlap.

The authoritative placement is cleared first:

- An existing clip fully covered by the new range is removed.
- An existing clip overlapping one edge is trimmed to the boundary.
- An existing clip spanning the new range is split around it.
- The edited clip is then inserted.

The lane order is maintained as `(range.start, ClipId)`.

This is deterministic auto-trim, matching the v0 plan: the edited clip wins and other clips yield.

## 5. Edit operations

- Move: changes track and/or range. Moving to another track requires kind compatibility: MIDI to Instrument, audio to Audio. Incompatible targets are not used by the UI.
- Trim start/end: adjusts `range`; for audio, the source sample window follows the trim. Trims preserve a non-empty range.
- Split: requires a strictly interior beat. The left clip keeps its `ClipId`; the right clip receives a new `ClipId`. Crossing MIDI notes are split with supplied new IDs. Where an audio clip uses slices, the right side receives new slice IDs.
- Mute: toggles `muted`; playback and rendering dim the clip.
- Delete clip: removes the entity and its lane reference.
- Delete time range: for each intersecting clip on the selected tracks:
  - fully covered: delete;
  - spans the range: split at the range start, then trim/delete the right part;
  - left overlap: trim the end to the range start;
  - right overlap: trim the start to the range end.
- Edit beyond the audio clip end: the UI extends the clip range before applying slices (`SetClipSlices`), so slice content can grow the clip.

## 6. Independent copies: no linked clips

Decision: the current implementation has no linked, ghost, alias, or shared-instance clips. There is no source-clip reference, link flag, or propagation mechanism in `ClipEntity`.

Every duplication/copy path materialises an independent clip:

- `Copy` + `Paste` creates new `ClipId`s.
- `Cmd+D` on selected clips creates new `ClipId`s and preserves group-relative offsets.
- Duplicate-on-clip creates a new `ClipId` placed after the source clip.
- Time-range duplicate copies trimmed clip content and pastes it at the range end.
- Split creates a new `ClipId` for the right side.
- Track duplication creates new clips, new clip IDs, and new note IDs.

What is shared and what is not:

- MIDI: payload is cloned and every note gets a new `NoteId`.
- Audio: payload is cloned, so range, slices, gain, and fades are independent per clip. The `asset_id` still points to the same read-only project asset; this is media sharing, not clip linking.
- Editing any copy never updates another copy, and there is no UI affordance to create or break a link.
- Track duplication is also independent: devices, child chains, mixer values, active state, route, and clips are copied to a new track.

Implementation note: audio slice IDs are not remapped when an audio clip is duplicated. Slice IDs are used as clip-local selection handles; no core lookup relies on global slice-ID uniqueness. If slice identity ever becomes globally addressable, duplication must remap them.

Linked clips are explicitly deferred. Adding them later requires a domain representation (for example a source-clip reference plus propagation rules) and a UI for link state; it must not be retrofitted by silently sharing payloads. This is separate from launcher-arranger aliasing, which is also outside the current scope.

## 7. Clipboard semantics

The arranger and the audio clip editor have separate clipboards.

### Arranger clipboard

- Node copy stores each selected clip with its track and an offset from the earliest selected clip's start.
- Time-range copy trims every intersecting clip on the selected tracks to the selection, storing offsets from the range start.
- Paste target: the start of an active time selection if one exists, else the playhead.
- Paste preserves the stored track IDs. Entries whose track no longer exists are skipped.
- Paste creates new `ClipId`s; MIDI notes receive new `NoteId`s.
- After paste, selection becomes the pasted clips, or the pasted time range for a time-range copy.
- The clipboard is in-memory app state. The system clipboard receives only the marker text `notwig-clipboard`.

### Audio clip editor clipboard

- Node copy stores slices with offsets from the earliest selected slice.
- Bar copy trims slices to the bar range and stores offsets from the range start.
- Paste target: the bar-selection start if present, else the insertion cursor.
- Paste creates new slice IDs and selects the pasted content.
- Bar duplicate places the clone at the bar end and selects the new range; node duplicate places clones after the selected set.

## 8. Duplicate targets

- Node duplicate (arranger): target is the maximum end of the selected clip ranges. Relative offsets are preserved, so the duplicate group begins immediately after the selection.
- Bar duplicate: target is the bar range end.
- Single clip duplicate: the new clip starts at the original clip's end.
- Track duplicate: inserted immediately after the source track, named `<source> copy`. It copies kind, parent, mixer, active state, audio route, device chain recursively, and clips. It does not copy track colour/comment, clip names/colours/mute state, or send configuration.

## 9. Invariants

1. A clip always has exactly one stable `ClipId`.
2. Clip ranges are non-empty and never negative.
3. Clip kind matches payload and host track.
4. Lanes contain only Audio/Instrument clips.
5. Accepted edits leave no overlapping clips on a lane.
6. Copies are independent; no edit propagates between clips.
7. Copy/paste/duplicate/split always allocate new identities for the new entity.
8. MIDI notes inside duplicated/pasted clips receive new note IDs.
9. Audio assets are shared read-only; clip edits never mutate them.
10. Linked clips do not exist in this implementation.
