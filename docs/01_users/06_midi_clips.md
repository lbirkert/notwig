# MIDI Clips

A MIDI clip contains notes with pitch, start, length, velocity, and optional
release velocity. Double-click a MIDI clip to open the MIDI editor, or select it
and use the clip editor button in the status bar.

## Layout

The MIDI editor has four areas:

- a piano keyboard header on the left;
- a ruler with the beat grid and loop range;
- a note area with one horizontal lane per pitch;
- a velocity lane along the bottom.

Notes outside the clip range are visible and editable. They do not play. The
clip start and end are controlled only by the clip handles, never by note
editing.

## Creating and deleting notes

- Double-click an empty area of the note grid to create a note.
- The new note snaps to the grid and gets a default length that does not
  overlap an existing note on the same pitch.
- Double-click a note to delete it.
- Click a piano key to preview that pitch. Dragging across keys keeps the
  preview held until you release the pointer.

Creating a note replaces any existing note it would overlap.

## Selecting notes

- Click a note to select it. Cmd-click or Shift-click adds notes.
- Drag on an empty area to draw a marquee. The marquee selects notes and can
  also create a time selection.
- Click an empty area to place a zero-width time selection.
- The velocity lane shows a bar for every visible note. Selected bars are drawn
  thicker.

## Editing notes

| Action | How |
| --- | --- |
| Move | Drag a note. It moves in time and pitch. |
| Resize start | Drag the note's left edge. |
| Resize end | Drag the note's right edge. |
| Nudge left or right | Arrow keys. The step follows the visible grid. |
| Transpose one semitone | **Up** or **Down**. |
| Transpose one octave | **Shift+Up** or **Shift+Down**. |
| Change velocity | Drag a velocity bar or click in the velocity lane. |
| Copy | **Cmd+C** |
| Cut | **Cmd+X** |
| Paste | **Cmd+V** |
| Duplicate | **Cmd+D** |
| Delete | **Delete** |

Moves and resizes are collision-aware: a note is trimmed instead of overlapping
another note on the same pitch. Hold **Shift** or **Alt** to bypass snapping.

The editor previews a note when you create it, select it, move it, or change its
velocity on the keyboard. Playing notes are highlighted while the transport
runs.

## Clipboard and time selection

The MIDI editor keeps its own note clipboard. Copy and paste work on notes and
time selections. Pasting uses the time selection start, or the playhead when no
time selection exists. Duplicate places the copy after the selection.

The arranger and audio clip editor share a different clipboard. MIDI note
clipboard data is not interchangeable with them.

## Clip boundaries and scrolling

- Drag the clip start or end handle to change the clip range.
- Dragging a note outside the clip range is allowed. The note stays visible and
  editable but stays silent during playback.
- Scroll horizontally with the scrollbar or a horizontal gesture.
- Scroll vertically over the pitch lanes to change the visible pitch range.
- Pinch or middle-drag to zoom.

The MIDI editor currently edits notes only. MIDI CC, pitch bend, aftertouch,
program changes, MIDI file import and export, and per-note expression editing
are not implemented.

Next: [Devices](07_devices.md).
