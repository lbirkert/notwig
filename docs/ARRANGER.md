# Arranger UI Contracts

Current implementation: `crates/notwig-ui/src/arranger.rs`, `drop_target.rs`, `timeline/`, and the shell in `shell.rs`.

Related: [`EDITING.md`](./EDITING.md) for time, viewport math, snapping, selection, and clip-drag rules; [`CLIPS.md`](./CLIPS.md) for clip identity and copy semantics.

## 1. Geometry

The arranger is the central panel. Its constants are:

- Track-header width: `min(230, 0.45 * available_width)`.
- Row height: 64 px.
- Ruler height: 30 px.
- Horizontal scrollbar height: 14 px.
- Master row: one 64 px row, always last, above the scrollbar.
- Group depth indent: `min(14 * depth, 0.35 * header_width)`.

The lane tree occupies the left header column; timeline content occupies everything to its right. The master row is fixed: it never scrolls vertically and never participates in track dragging. It remains clickable.

The grid is drawn once over the whole timeline content area, behind all rows and clips, not once per lane.

See [`EDITING.md`](./EDITING.md) for the full geometric and viewport formulas.

## 2. Track rows and headers

Tracks render in depth-first order. Each row has:

- A 4 px colour stripe. The colour is the track's custom colour, else a kind colour.
- Track name, clipped to the available header width.
- Record dot. It is visual only; recording is not implemented.
- Mute and solo buttons, which emit mixer changes.
- A kind icon.
- A volume fader.
- A stereo meter fed from runtime peaks.
- A three-line menu glyph. It is presentational; the actual menu is opened by right-clicking the header.

Clicking the header selects the track. `Cmd`/`Shift` toggles additive selection.

### Track header context menu

- Group selected tracks.
- Add audio track.
- Add instrument track.
- Add group track.
- Delete track, except Master.

### Add-track row

A final row below the track list offers `+ Add track`. A plain click creates an audio track. Its context menu creates audio, instrument, or group tracks.

### Track dragging

Track reordering uses row centres:

- Pointer above a row centre: insert before that row.
- Pointer at or below: insert after it.
- Dropping after the last row inserts at the end.
- Parent and sibling index are inherited from the neighbouring row.
- Dropping a track into itself or any descendant is invalid.
- Master is not draggable.
- A white insertion line and dot show the drop position.

The draggable region is the track name/top area, not the whole header.

## 3. Ruler and play start

The ruler renders:

- Bar/beat lines from the shared adaptive grid.
- Bar labels and beat labels at sufficient zoom.
- The loop range overlay when loop is enabled.
- The time-selection overlay when a bar selection exists.
- A triangular play-start marker.

Interactions:

- Click or drag the ruler to set the play-start marker at the pointer's snapped beat.
- `Shift` bypasses snapping.
- While playing, setting the play-start marker does not move the playhead.
- While stopped, setting it also locates the playhead.
- There is no direct loop-range drag in the current implementation. `Cmd+L` applies a time selection as the loop range, or toggles loop if no time selection exists.

## 4. Lane interactions

- Clicking an empty lane creates an empty time selection on that track. It does not create a clip.
- Dragging in the lower half of a lane creates a time selection. Dragging vertically extends the selection across every row it crosses.
- `Shift` bypasses snapping for lane selections.
- Double-clicking an empty instrument lane creates a 4-beat empty MIDI clip at the snapped beat.
- Double-clicking an empty audio lane inserts the first imported audio asset at the current playhead. If no audio asset exists, it posts an informational notification.
- Right-clicking a non-master lane offers `Add MIDI clip at playhead`. Audio lanes additionally offer `Insert audio asset` with a list of project assets.

An empty time selection is cleared when the drag ends.

## 5. Clip display

A clip is drawn as a coloured rectangle with a darker title strip:

- Audio clips use blue; MIDI clips use green.
- Muted clips are dimmed.
- The title strip shows the clip name or a generated summary (`MIDI · N notes` / `AUDIO · N.Ns`).
- The body renders MIDI note previews or audio slices/waveforms.
- Selected clips get a thick warm-yellow outline.
- Drag previews get an accent outline.
- A clip moved to a different track shows a preview outline on the target lane.

Resize handles exist only on the title strip's left and right edges; they are not full-height.

Interactions:

- Click title strip: select clip; `Cmd`/`Shift` toggles additive selection.
- Drag title strip: move the selection. If several clips are selected and the pressed clip belongs to the selection, the whole group moves.
- Drag title edge: trim start or end.
- Double-click audio clip title strip: open the audio clip editor.
- Right-click: Mute/Unmute, Duplicate, Split at playhead (only when the playhead is strictly inside the clip), Delete.

Move/trim geometry and snapping are in [`EDITING.md`](./EDITING.md).

## 6. Time selection rendering

A time selection is shown in the ruler and on every selected lane as a translucent warm overlay. Start and end markers draw as vertical lines across the content. Dragging the selected region moves the selection and intersecting clips on its tracks as one group.

## 7. Horizontal navigation

- Pixels per beat is clamped to 4-800.
- Pinch/zoom gestures zoom around the pointer.
- Middle-drag pans horizontally; its vertical component zooms horizontally around the pointer.
- Horizontal scroll pans.
- The scrollbar maps the viewport over content ending at the latest clip end (minimum 8 beats, plus padding).
- `Cmd+0` fits the arrangement: start at zero and choose a pixels-per-beat that covers the latest clip end plus one beat.

Zooming always preserves the beat under the pointer. Panning never moves the viewport start below zero.

## 8. Audio drag-and-drop

The target resolver uses the current row rectangles:

- Within 7 px of a row edge: new track at the boundary.
- Inside an audio lane: that track.
- Inside a non-audio lane: new audio track after it.
- Below all rows: append a new audio track.

While hovering, the arranger draws an insertion line for a new track or highlights the target lane and shows a ghost clip. The ghost uses the snapped drop beat and the decoded file duration. See [`FRONTEND.md`](./FRONTEND.md) for multi-file and save-first behaviour.

## 9. Invariants

1. Master is always visible, pinned above the scrollbar, clickable, and never draggable.
2. The grid is rendered once over timeline content, not per lane.
3. Track headers and lanes are distinct hit regions; the header drag area does not cover the Master or its controls.
4. Clips may only target compatible tracks: MIDI to Instrument, audio to Audio.
5. Move and trim use pointer differences from the press origin, never per-frame deltas.
6. A track may not be dropped into itself or a descendant.
7. Lane double-click creates a 4-beat MIDI clip or inserts an audio asset; it does not open a note editor.
