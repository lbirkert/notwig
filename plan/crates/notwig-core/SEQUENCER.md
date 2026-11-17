# Sequencer / Arranger (`notwig-core`)

## 1. Scope
- Linear Arranger timeline only (v0). **Excluded:** Clip Launcher, scenes, follow actions, launcher-arranger aliasing, comping/takes, audio warping, recording (audio/MIDI), live input, and content loop/repeat.
- Clips exist only on `Audio` and `Instrument` tracks; Group/BusFx/Master have no lanes.

## 2. Model
`ArrangerState = { lanes: BTreeMap<Id<Track>, Lane> }`
`Lane = { clips: Vec<Id<Clip>> }` (sorted by `(range.start, id)`; maintained by Core; only for Audio/Instrument tracks)

`Clip = {
  id: Id<Clip>,
  trackId: Id<Track>,
  kind: Midi | Audio,
  range: RangeMus,                // [start,end) on arranger timeline
  name: Option<String>,
  color: Option<Color>,
  muted: bool,
  payload: ClipPayload
}`

`ClipPayload = Midi(MidiPayloadRef) | Audio(AudioPayloadRef)`
- Clip ranges MUST be non-empty (`range.end > range.start`); splitting at an existing boundary is a no-op/rejected.

## 3. MIDI Payload
`MidiPayloadRef -> MidiClipData { length: TimeMus, contentStart: TimeMus, notes: Vec<Note> }`
`Note = {
  id: Id<Note>,
  pitch: u8 (0..127),
  start: TimeMus,                 // relative to content origin, >=0
  length: TimeMus,                // >0; note.start + note.length <= content length
  velocity: Norm01,
  releaseVelocity: Option<Norm01>,
  expression: BTreeMap<String, ExprValue> // reserved extensible per-note expression (not captured/edited in v0)
}`
- `length` is content length; `contentStart >= 0` is the window offset into content. Notes MUST lie within `[0, length)`; overlapping notes are allowed (voice semantics belong to the instrument).
- Notes only in v0: no MIDI channel, CC, pitch bend, program change, sysex, or note-expression capture. Reserved fields MUST NOT affect playback.
- Clip `range` windows content from `contentStart`: visible/played content is `[contentStart, contentStart + range.length)` clipped to `[0, length)`. Content never repeats.
- Left-trim advances `range.start` and `contentStart` equally (head content hidden, not destroyed); right-trim adjusts `range.end` only. Extending left is limited by `contentStart >= 0`; extending right beyond content yields an empty tail. Notes partially outside the window are cut at playback boundaries.

## 4. Audio Payload
`AudioPayloadRef -> AudioClipData {
  assetId: Id<Asset>,
  sourceStartSample: u64,
  sourceEndSample: u64,           // > start; asset is at project sample rate
  gainDb: Db,
  fadeInSamples: u64,
  fadeOutSamples: u64,
  fadeShape: Linear                // enum reserved for future shapes
}`
- v0: non-destructive trim/fades/gain only. No warp markers, stretch modes, elastic tempo-follow, or content looping.
- `clip.range` length, converted at the constant tempo, MUST equal `sourceEndSample - sourceStartSample`; trims snap to sample boundaries and couple the timeline range and source boundaries. Left-trim advances `range.start` and `sourceStartSample` equally; right-trim adjusts `range.end` and `sourceEndSample`. Extending is only possible where source material exists (no silent tail).
- Fades are linear, clamped to the source region, and MUST NOT overlap (`fadeInSamples + fadeOutSamples <= source duration`).

## 5. Edit Operations & Invariants
- Clip create/move/trim/resize/split/duplicate/delete/mute/rename operate on `Id<Clip>` and validated ranges.
- Create/duplicate/split commands carry caller-generated `Id<Clip>`/`Id<Note>` values; Core never allocates IDs. v0 duplicate is clip-only; track/device duplication and cross-project copy/paste are deferred.
- Track compatibility: MIDI clips only on Instrument tracks; audio clips only on Audio tracks. Cross-track moves must satisfy kind compatibility.
- Split positions snap to sample boundaries for audio; notes crossing a MIDI split are divided at the split using the supplied `newNoteIds` mapping.
- Referential integrity: `clip.trackId` exists and lane contains clip; payload refs and note bounds valid; `clip.kind` matches the payload variant and track kind.
- `contentStart <= contentLength`; reducing `contentLength` below the furthest note end is rejected unless the same command trims the affected notes.
- **Overlap policy (v0, Bitwig-like auto-trim):** clips on the same lane MUST NOT overlap after an accepted edit.
  - Applying an edit that introduces overlap auto-trims/removes affected existing clip(s) deterministically so resulting lane has disjoint `[start,end)` ranges.
  - Determinism rule: edited/moved/resized/split-result clip(s) are the authoritative placement; intersecting prior clips are trimmed at intersection boundaries or removed if fully covered. Ordering by clip IDs used only as stable tiebreak where otherwise ambiguous.
- MIDI edits: add/remove/move/resize/velocity change for notes within clip constraints.

## 6. Playback/Scheduling Contract
- Engine derives the active set by playhead intersection with clip `range` in musical time via the constant tempo map (v0).
- MIDI clips emit note-on/note-off events only to the owning hosted instrument; clip mute suppresses emission. There is no note chase in v0: starting/locating mid-note does not emit a note-on; stop/pause/locate emits all-notes-off. At equal sample offsets, note-offs precede note-ons.
- Audio clips stream/decode the asset source region with clip gain and linear fades applied; source assets are never destructively modified.
- Loop playback: when enabled, playhead wraps `loopRange`; clip activation follows resulting playhead intervals (deterministic boundary handling) and loop wrap emits all-notes-off before resuming.
- Recording/live input playback paths are absent in v0.
