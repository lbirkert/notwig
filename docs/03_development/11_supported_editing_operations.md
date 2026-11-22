# Supported editing operations

Status: audit snapshot of the arranged, audio clip, and MIDI editors. It records
what each editor exposes today, including operations that are deliberately
missing. The shared algebra and its test contract are described in
[`Editing`](03_editing.md) and [`Editing Testing`](04_editing_testing.md).

## 1. How to read this

| Icon | Meaning |
| --- | --- |
| ✅ | Supported. A direct gesture or shortcut reaches it, and it is covered by the shared algebra or editor tests. |
| 🧪 | Partial or experimental. It works only in one selection mode, only through an indirect gesture, or lacks a dedicated command or UI. |
| ❌ | Not supported. There is no UI path or command today. |
| — | Not applicable to that editor's object model. |

"Object" means a whole selected object. "Time" means a time-range selection.
When a cell says "Time / Object", both selection modes work.

## 2. Selection

| Editing operation | Arranger | Audio Editor | MIDI Editor | Notes |
| --- | --- | --- | --- | --- |
| Select object | ✅ Object | ✅ Object | ✅ Object | Clip, audio event, and note. |
| Additive select | ✅ | ✅ | ✅ | Cmd-click or Shift-click toggles an object. |
| Marquee select | ❌ | ❌ | ✅ Object + Time | The MIDI marquee selects notes and forms a time selection. Arranger marquee selection is still planned. |
| Select all | ✅ | ✅ | ✅ | Cmd+A selects every clip, audio event, or note in the focused editor. |
| Invert selection | ❌ | ❌ | ❌ | No command or intent. |
| Create time selection | ✅ | ✅ | ✅ | Drag an empty lane or clip body. MIDI click on empty space creates a zero-width range. |
| Move time selection | ✅ | ✅ | ❌ | Arranger and audio drag the selection body and move its content. MIDI has no drag-move; its range shifts only as a side effect of note nudges and duplicate. |
| Trim time selection edges | ✅ | ❌ | ❌ | Arranger start and end handles are implemented in `arranger/selection.rs`. The roadmap line that calls this planned is stale. |
| Clear selection | ✅ | 🧪 | 🧪 | Arranger clears with Escape or an empty click. Clip editors mostly leave a zero-width time selection instead, and Escape closes the editor. |

## 3. Object and clipboard editing

| Editing operation | Arranger | Audio Editor | MIDI Editor | Notes |
| --- | --- | --- | --- | --- |
| Copy | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Arranger and audio share one clipboard. MIDI keeps a separate note clipboard. |
| Cut | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Copy, then delete. The clipboard changes only after lowering succeeds. |
| Paste | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Pastes at the time selection start or the playhead. Destination content is cleared first. |
| Duplicate | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Places the copy after the selection and selects it. MIDI uses the time selection length as the offset. |
| Delete | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Object mode removes whole objects. Time mode erases, trims, or splits intersecting objects. |
| Move object | ✅ Time / Object | ✅ Time / Object | ✅ Time / Object | Arranger can move across compatible tracks. Audio moves in time inside one clip. MIDI moves in time and pitch. |
| Trim object edges | ✅ | ✅ | ✅ | Clip edge, audio event edge, and MIDI note edge. Audio and MIDI also expose clip-boundary handles. |
| Create object | ✅ | 🧪 | ✅ | Arranger double-click creates a clip. MIDI double-click creates a note. Audio events are created only by dropping audio files; there is no empty-event draw tool. |
| Mute object | ✅ | ❌ | ❌ | Arranger clips can be muted from the clip menu. Audio events and MIDI notes have no mute; clip-level mute is available through the info panel. |
| Split at playhead | ✅ | ❌ | ❌ | Arranger clip context menu only. |
| Split at time-selection point | ✅ | ✅ | ✅ | Delete on a zero-width time selection splits crossing clips, audio events, or notes. There is no scissors tool. |
| Join adjacent objects | ✅ | ❌ | ❌ | Cmd+J joins same-lane, same-kind, adjacent clips. With a time selection, the intersected clips are used when fewer than two clips are explicitly selected. Works for audio and MIDI clips. |
| Undo / redo | ✅ | ✅ | ✅ | Workspace-level. One user operation is one undo group. |

## 4. Editor-specific operations

| Editing operation | Arranger | Audio Editor | MIDI Editor | Notes |
| --- | --- | --- | --- | --- |
| Nudge left or right | ❌ | ❌ | ✅ | Arrow keys move selected notes by the visible grid step. |
| Transpose | — | — | 🧪 | Up and Down transpose one semitone; Shift+Up and Shift+Down transpose one octave. There is no scale or transpose dialog. |
| Velocity edit | — | — | ✅ | Drag or click in the velocity lane. |
| Nudge in pitch | — | — | ✅ | Up and Down also change pitch. |
| Preview note | — | — | ✅ | Piano keys, note clicks, moves, and velocity edits preview audio. |

## 5. Not implemented anywhere

These are unsupported across all three editors. They are either deferred or
tracked in [`Roadmap`](06_roadmap.md).

| Editing operation | Arranger | Audio Editor | MIDI Editor | Notes |
| --- | --- | --- | --- | --- |
| Quantize | ❌ | ❌ | ❌ | No note or event quantization. |
| Groove, swing, humanize | ❌ | ❌ | ❌ | No timing-feel tools. |
| Scale and chord tools | ❌ | ❌ | ❌ | Keyboard transposition exists; scale-aware tools do not. |
| Fade editing | ❌ | ❌ | — | Audio fades are stored and rendered, but no UI edits them. |
| Crossfade | ❌ | ❌ | — | No crossfade gesture or command. |
| Reverse | ❌ | ❌ | — | No reverse command. |
| Time stretch and warping | ❌ | ❌ | ❌ | Audio is sample-locked to the current tempo. |
| Clip loop and repeat | ❌ | ❌ | ❌ | No loop or repeat handle. |
| Ripple edit | ❌ | ❌ | ❌ | Deleting time does not close the gap. |
| Automation editing | ❌ | ❌ | ❌ | The automation adapter and editor are not wired. |
| Paste to another lane or track | ❌ | ❌ | ❌ | Paste keeps every fragment on its source lane. |
| Multi-object trim | ❌ | ❌ | ❌ | Object trim is single-object; time-edge trim resizes the selection instead. |
| Cross-project paste | ❌ | ❌ | ❌ | Deferred until project-level identity mapping exists. |

## 6. Findings

1. **The core operation set is genuinely shared.** Copy, cut, paste,
   duplicate, delete, move, and trim all run through
   `crates/notwig-core/src/edit/`. Each editor supplies an `EditModel` and a
   lowering step; none keeps a second copy of the operation math.

2. **Selection is shared through the editors, including Select All.** Cmd+A is
   a `SelectAll` command. The focused editor chooses the object set: clips in
   the arranger, events in the audio editor, and notes in the MIDI editor.

3. **Selection-gesture coverage is uneven.** MIDI has a real marquee. The
   arranger and audio editor can only create time selections by dragging, and
   selecting several objects relies on additive clicks. The roadmap still lists
   arranger marquee selection as planned.

4. **Time selections are not symmetric.**
   - Arranger: create, move, and trim both edges.
   - Audio editor: create and move, but no edge handles.
   - MIDI editor: create, but no direct move or edge trim. The range is used as
     a clipboard and duplicate anchor and follows note nudges.

5. **Point split works in every editor; join is arranger-only.** Delete on a
   zero-width time selection emits `EditOp::Split` for crossing clips, events,
   or notes. Cmd+J joins adjacent arranger clips, using the time selection when
   no two clips are explicitly selected. Neither clip editor has a join command.

6. **Object creation differs by editor.** Arranger and MIDI create objects with
   a double-click. The audio editor creates events only by dropping audio
   files, so there is no way to draw an empty event.

7. **MIDI owns the note-tooling.** Keyboard nudge, semitone/octave transpose,
   and velocity editing exist only in the MIDI editor. Quantize, groove, and
   scale tools are still absent.

8. **Cross-editor paste is rejected by design.** The extract carries an
   `EditorKind` domain tag, and paste validates it. This is not a missing
   feature; converting notes into clips or slices into clips would be a
   separate feature.

9. **Clip fades and gain are engine features without an editor.** The data model
   and renderer support them, but no editing UI exists. They are outside the
   shared edit algebra.

10. **The roadmap has one stale entry.** `06_roadmap.md` says arranger time
    selection edge trim is planned, but `arranger/selection.rs` implements
    draggable start and end handles. The roadmap row should be updated.

## 7. Where it lives

- Algebra and arranger model: `crates/notwig-core/src/edit/mod.rs`,
  `crates/notwig-core/src/edit/algebra.rs`,
  `crates/notwig-core/src/edit/arranger.rs`.
- Arranger lowering and workspace operations:
  `crates/notwig-app/src/edit_lowering.rs`,
  `crates/notwig-app/src/workspace/mod.rs`.
- Arranger gestures: `crates/notwig-ui/src/views/arranger/clip.rs`,
  `crates/notwig-ui/src/views/arranger/selection.rs`,
  `crates/notwig-ui/src/views/arranger/track.rs`.
- Audio editor: `crates/notwig-ui/src/views/audio_editor/actions.rs`,
  `panel.rs`, `model.rs`, and `geometry.rs`.
- MIDI editor: `crates/notwig-ui/src/views/midi_editor/actions.rs`,
  `panel.rs`, and `model.rs`.
- Shortcut dispatch: `crates/notwig-ui/src/views/shell.rs` and
  `crates/notwig-ui/src/commands.rs`.
- Coverage: `crates/notwig-core/tests/edit_algebra.rs`,
  `crates/notwig-ui/tests/audio_editor.rs`,
  `crates/notwig-ui/tests/midi_editor.rs`,
  `crates/notwig-app/tests/workspace.rs`, and
  `crates/notwig-ui/tests/shell_internal.rs`.
