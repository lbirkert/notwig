# Audio Clips

An audio clip references an asset and contains one or more audio events. The
arranger shows the clip on its audio track. The audio clip editor shows the
events inside the clip and the clip's own start and end boundaries.

You can create an empty audio clip before importing any audio. It stays silent
until you place an event in it, for example by dragging an audio file into the
clip editor.

Open the audio clip editor by double-clicking an audio clip, or select the clip
and use the clip editor button in the status bar.

## The clip and its events

The clip range is the part of the timeline occupied by the clip. Events are
source windows placed inside or outside that range.

- Dragging the **clip start** or **clip end** handle changes the clip range.
- Event edits never move those handles. Paste, duplicate, move, and trim can
  place events before the clip start or after the clip end.
- Events outside the clip range stay visible, selectable, and editable, but they
  do not play. Playback always follows the clip range.
- The waveform is drawn where the source has samples. Over-trimmed events show
  an empty tail instead of a stretched waveform.

This separation lets you prepare material outside the playing section without
losing it.

## Selecting events

- Click an event to select it. Cmd-click or Shift-click adds events.
- Drag on an empty part of the editor or across an event body to create a time
  selection.
- Drag the time selection to move its content.
- Hold **Shift** while selecting or moving to bypass snapping.
- Click an empty area to clear the selection.

## Editing events

| Action | How |
| --- | --- |
| Move | Drag the event's title bar horizontally. |
| Trim start | Drag the event's left edge. |
| Trim end | Drag the event's right edge. |
| Copy | **Cmd+C** |
| Cut | **Cmd+X** |
| Paste | **Cmd+V** |
| Duplicate | **Cmd+D** |
| Delete | **Delete** |

Trim advances or reduces the source window; it never stretches the audio. A
start trim moves the first audible sample, and an end trim changes the last
sample. Event moves and trims snap the event's **absolute timeline position** to
the visible grid, even when the clip itself starts between grid lines. Hold
**Shift** or **Alt** to bypass snapping.

Paste places the clipboard at the time selection start, or at the playhead when
there is no time selection. The target is an absolute timeline position, so an
off-grid clip start does not shift pasted events off the grid. Destination
content is cleared first. Pasting an
arranger selection into the audio editor is rejected because the clipboard
belongs to a different editor.

## Clip gain and fades

Audio clips store gain, optional gain and pan curves, and linear fades, and the
engine renders them. The **Gain** sub-editor exposes the gain curve as a
reusable line editor:

- hover the curve to show the add-point cursor, then click the line to insert
  and drag a point immediately;
- drag a point to move it in time and value (`Shift` uses fine time and value
  steps and bypasses snapping; toggling it mid-drag keeps the movement already
  made and starts a new offset from the current point);
- double-click a point to delete it;
- drag a point with **Cmd/Ctrl** or **Shift** to add it to the selection;
- drag empty space to marquee-select points and set a time selection; clicking
  empty space shrinks the time selection to a zero-width range;
- select a point normally to fit the time selection to that point, or
  **Shift/Cmd**-click to add it while keeping the previous time selection;
- press **Backspace** to delete selected points;
- use the normal audio-editor copy, cut, paste, and duplicate shortcuts on the
  selected points; duplicate uses the time-selection length as its offset and
  paste starts at the time-selection start;
- **Option/Alt**-drag a segment to bend its curvature: positive bends ease in,
  negative bends ease out; holding Option/Alt over a segment shows the curvature
  drag cursor;
- **Cmd/Ctrl**-drag a segment to move both endpoint values up or down; holding
  Cmd/Ctrl over a segment shows the vertical move cursor.

The curve is an offset in dB on top of the clip gain and any per-event gain.
Gain points may sit before the clip start or after the clip end, just like audio
events; playback holds the nearest endpoint outside the point range. The gain
curve belongs to the audio clip and is copied with clip copy/duplicate.
Gain edits are previewed on the waveform and through playback while you drag,
then committed as one undo entry on release.
The waveform behind the curve is scaled by the curve locally, so the displayed
audio follows the gain shape. The same locally scaled waveform is used in the
arranger and the audio editor. The line editor is shared UI code and is ready
for automation and other modulatable values.

## Clip pan

The **Clip Pan** sub-editor is supplied by the example extension. It uses the
same global curve editor as Gain, so every gesture, shortcut, selection rule,
and undo boundary is identical:

- click the center line to add and drag a pan point;
- drag points in time and value, or bend segments to shape the pan curve;
- copy, cut, paste, duplicate, delete, and select all use the audio editor's
  point selection;
- points may sit outside the clip; playback holds the nearest endpoint.

Pan values run from hard left (`-1`) through center (`0`) to hard right (`1`).
The hover label shows `L`, `C`, or `R` percentages. A missing pan curve, or a
flat center curve, leaves the clip unchanged. Clip pan curves are saved with the
project and copied with the clip.

Fade shapes, crossfades, reverse, and time stretching are not implemented.

## Waveform and navigation

The waveform is drawn from cached signed min/max data. When you zoom in far
enough, it switches to a graph of the individual samples, so you can see the
waveform rise and fall around zero rather than only its absolute peak. Zoom and
pan use the same gestures as the arranger:

- pinch to zoom;
- middle-drag horizontally to pan, vertically to zoom;
- horizontal scrollbar to scroll.

The clip editor may pan left of the zeroth bar so events and gain points placed
before bar zero can be inspected and edited.

You can zoom in to sample-level detail. The editor draws the grid and ruler
across the visible range, including areas outside the clip. The content shading
shows where the clip actually plays.

## What happens at the edges

- Trimming an event at the start of its source can only reveal samples that
  exist in the asset. It cannot invent a silent prefix.
- A fully covered event is removed when the clip range no longer contains any of
  it.
- The clip start and end cannot invert. The editor clamps to a minimum length.

Next: [MIDI Clips](06_midi_clips.md).
