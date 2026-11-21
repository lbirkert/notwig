# Arranger

The arranger is the main timeline. It shows track headers on the left and
MIDI and audio clips on the right, ordered by beat.

## Ruler, playhead, and loop

- Click the ruler to set the play start. While playback is running, this sets
  where playback will return when you stop.
- Shift-drag the ruler to set the loop range.
- Click the loop button in the top bar to enable or disable looping.
- With a time selection, **Cmd+L** sets the loop to that range. With clips
  selected and no time selection, it sets the loop to their combined bounds.
  With neither, it toggles the loop.

The playhead moves during playback. Stop returns to the loop start when looping
is enabled, otherwise to the play start. Playback wraps at the loop end with
sample accuracy.

## Creating clips

- Double-click an empty lane to create a four-beat clip. Audio tracks create
  audio clips; instrument tracks create MIDI clips.
- Double-clicking stops at the next existing clip, so a new clip does not
  overlap one that follows.
- Drag an audio file from Finder onto an audio track to import and place it.
- Right-click a lane and choose **Add MIDI clip at playhead** or **Insert audio
  asset**.
- Right-click **+ Add track** or a track header to create a new track.

## Selecting and moving

- Click a clip to select it. Cmd-click or Shift-click adds clips to the
  selection.
- Drag a clip's title bar to move it in time. Drag it vertically onto a
  compatible track to move it across tracks. MIDI clips accept instrument
  tracks; audio clips accept audio tracks.
- Drag across a clip body to create a time selection without moving the clip.
- Drag a group selection to move all selected clips by the same amount. Group
  moves keep relative offsets and stay to the right of beat zero.
- Hold **Alt** or **Shift** while dragging to bypass snapping.
- Drag a clip's left or right edge to trim it. Trimming changes the clip range
  and its content window; it never stretches audio.

## Time selection

- Drag on an empty lane or across clip bodies to create a time selection.
- Drag vertically while creating one to include more tracks.
- Drag the selection itself to move it, together with its content, in time.
- Drag either selection edge to extend or shorten the time range.
- Hold **Shift** while selecting, moving, or resizing to bypass snapping.
- Click an empty lane once to clear the selection.

A time selection is the input for copy, cut, paste, duplicate, and delete. The
loop range command also accepts a clip selection when no time selection exists.

## Clipboard and structure

The arranger and the audio editor share one workspace clipboard.

| Action | Key | Notes |
| --- | --- | --- |
| Copy | **Cmd+C** | Copies the selected clips or time range. |
| Cut | **Cmd+X** | Copies, then deletes the source. |
| Paste | **Cmd+V** | Pastes at the time selection start or the playhead. |
| Duplicate | **Cmd+D** | Copies and places after the selection. |
| Delete | **Delete** | Deletes the selected clips or time range. |
| Join clips | **Cmd+J** | Joins selected adjacent clips on one lane. |
| Split at playhead | Context menu | Splits a clip where the playhead crosses it. |
| Undo / redo | **Cmd+Z** / **Cmd+Shift+Z** | One user operation is one undo step. |

Paste and duplicate clear the destination before placing content. Existing
content that overlaps a destination is trimmed or split, never overlapped.
Pasting audio content into the arranger after copying from the audio editor is
rejected; the clipboard keeps its source editor.

Duplicate behavior depends on the selection:

- with a time selection, the copy is placed after the selection and the same
  length is selected;
- with an object selection, the copy is placed after the bounding end of the
  selected clips.

## Clip actions

Right-click a clip for:

- **Mute** or **Unmute**;
- **Duplicate**;
- **Split at playhead**, when the playhead is inside the clip;
- **Delete**.

Double-click a clip to open its editor. Audio clips open the audio clip editor;
MIDI clips open the MIDI editor.

## Zoom and navigation

- Pinch to zoom around the pointer.
- Middle-drag horizontally to pan and vertically to zoom.
- Two-finger horizontal scroll pans.
- **Cmd+0** fits the whole arrangement.
- Use the horizontal scrollbar below the lanes to scroll precisely.

The compact snap controls sit at the right end of the horizontal scrollbar and
appear in the same place in both clip editors. The magnet button toggles
snapping for every editor; when it is off, the grid is dimmed. The grid label
opens grid settings:

- **Adaptive** follows the zoom level and picks the finest subdivision that
  still leaves at least 40 px between grid lines.
- With adaptive off, choose an explicit note value from **2/1** down to
  **1/65536** (including **1/8**).

Hold **Shift** or **Alt** to bypass snapping for a gesture.

Next: [Audio Clips](05_audio_clips.md).
