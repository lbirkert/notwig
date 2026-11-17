# Frontend Contracts

Current implementation: `crates/notwig-ui/src/shell.rs`, `state.rs`, `intent.rs`, `info.rs`, `widgets.rs`, `theme.rs`, and `crates/notwig-app/src/gui.rs`.

Related: [`EDITING.md`](./EDITING.md) for time/selection/editing math, [`ARRANGER.md`](./ARRANGER.md) for track and lane UI, [`DEVICES.md`](./DEVICES.md) for the device rack, [`CLIPS.md`](./CLIPS.md) for clip and copy semantics.

## 1. Authority and state ownership

The UI is a projection over `ProjectDoc` (`S_doc`) and `RuntimeView` (`S_rt`). It never mutates the document directly.

- Every user action becomes a `UiIntent`.
- `notwig-app` translates intents into `CommandPayload`/workspace operations.
- Accepted edits are reflected back through the document; the UI does not keep a second editable copy of semantic state.
- Transient values live in UI state only: selection, primary items, viewports, drag state, panel visibility, expanded/collapsed devices, analyzer options, plugin search text, last-touched parameter, notifications, and edit focus.

Decision: UI-only state is not persisted in the project bundle. It is rebuilt or defaults after load.

### Undo/redo

Undo/redo is implemented in the app, not the UI.

- Each accepted non-transport command records a full `ProjectDoc` snapshot before applying it.
- History is capped at 200 snapshots.
- A new edit clears the redo stack.
- Transport commands (`SetTransport`, `Pause`, `Stop`) do not create undo entries.
- `Cmd+Z` / `Cmd+Shift+Z` emit `Undo` / `Redo`.
- Undo restores the document and rebuilds the engine; selection is then sanitised against the restored IDs.
- Continuous parameter gestures may emit several commands during one drag, so undo may step through intermediate parameter values.

## 2. Shell layout

The window uses `egui` + `wgpu`. Default size is 1440x900; minimum is 900x600.

Layout, in paint/order terms:

1. Top transport bar, 42 px high.
2. Bottom status bar, 40 px high.
3. Left inspector/info panel, 250 px wide, visible by default.
4. One bottom editor slot, either:
   - the device panel, 260 px high by default, or
   - the audio clip editor, 420 px high by default (resizable 260-700 px).
5. Arranger in the central panel.
6. Floating windows above the shell: add-device browser, notifications, help.

Decision: the device panel and the clip editor are mutually exclusive. The shell paints exactly one of them.

- Opening the clip editor remembers whether the device panel was open, hides the device panel, and sets edit focus to the clip editor.
- Closing the clip editor restores the previous device-panel visibility and returns focus to the timeline.
- The device panel is not painted while the clip editor is open, even if its own visibility flag is set.

The inspector can be hidden with the info button in the status bar or by dragging the right-edge collapse handle left.

## 3. Transport bar

The top bar contains, left to right:

- `FILE` menu.
- Play/Pause button.
- Stop button.
- Record button.
- Loop toggle.
- Centered engine strip: CPU meter, tempo, time signature, playhead readouts, master meter.

### File menu

- New Project (48 kHz)
- New Project (44.1 kHz)
- New Project (96 kHz)
- Open Project...
- Save
- Save As...
- Import Audio...
- Render Mixdown...

Importing audio requires a saved bundle. Dropping an audio file onto the timeline while the project is unsaved opens Save As first and queues the drop.

### Transport decisions

- The play button toggles Play/Pause based on runtime playback state.
- Stop returns to the play-start marker captured when playback started.
- The play-start marker is set by clicking/dragging the arranger ruler. While playing, setting it does not move the playhead; while stopped, it also locates the playhead.
- Enabling loop with no existing range creates a 4-beat range at `0`.
- The record button is a deliberately inert placeholder with the tooltip `Record (not implemented)`.
- Tempo and time signature are editable in the current build, even though older plan documents describe them as read-only.
  - Tempo: 20-400 BPM, 0.1 step, two decimals.
  - Time signature numerator: 1-32.
  - Time signature denominator: 1, 2, 4, 8, 16, or 32.
- The playhead is displayed both as bars.beats.sixteenths.ticks and as minutes:seconds.milliseconds.
- The master meter reads the master track from the runtime meter map.

## 4. Inspector / mixer panel

The left panel is selection-driven. Its content is chosen by this precedence:

1. If only track selection exists (no primary device, clip, or time selection), show the track inspector.
2. Otherwise show, in order: primary device, primary clip, time selection, or a "no selection" message.

### Track inspector

- Multi-track selection shows the count and a selectable list used to change the primary track.
- Shows track kind, activate/deactivate power control, editable name, colour palette (30 swatches plus clear), and comment field.
- Lists the track's main-chain devices. Clicking a device selects it; the power button toggles bypass.
- Mixer controls: record placeholder, solo, mute, pan, peak readout, stereo meter, and vertical volume fader.
- Track colour and comment edits emit intents immediately.

### Time-selection inspector

Shows start, end, length, lane count, and the selected lanes with their track kinds.

### Clip inspector

Shows clip name, kind (MIDI/AUDIO), owning track, start, length, and payload summary. MIDI shows the note count; audio shows source duration and fade lengths. Actions: mute toggle, duplicate, delete.

### Device inspector

Shows device name, type ID, bypass state, and either the EQ band overview/analyzer settings or the first 12 generic parameters. Details are in [`DEVICES.md`](./DEVICES.md).

### Record controls

The record button in the inspector is also inert (`Record (not implemented)`), consistent with deferred recording.

## 5. Bottom editors

### Device panel

Shows the selected track's main device chain. The panel is horizontally scrollable and is documented in [`DEVICES.md`](./DEVICES.md).

### Audio clip editor

The clip editor opens only for audio clips.

- Double-click an audio clip's title strip, or use the clip button in the status bar while an audio clip is selected.
- It replaces the device panel in the bottom slot.
- It has its own selection, clipboard, viewport, cursor, and bar selection.
- Editing rules are in [`EDITING.md`](./EDITING.md).

There is currently no piano-roll or MIDI note editor. MIDI clips can be created, moved, trimmed, split, duplicated, muted, and deleted in the arranger; their notes are only rendered as a preview in the clip body.

## 6. Status bar

The bottom status bar contains:

- Info toggle for the inspector.
- `ARRANGE` mode label in the accent colour.
- `MIX` and `EDIT` labels in the muted colour. These are placeholders; there is no mixer view or separate edit-mode surface.
- Grid, Share, and Monitor icons. They are placeholders and do not dispatch actions.
- Device-panel toggle.
- Clip-editor toggle. It opens only when a primary or selected audio clip exists.
- A `CLICK <Show/Hide Device Panel>` text action.

## 7. Focus and selection routing

Selection is documented in [`EDITING.md`](./EDITING.md). Frontend routing adds edit focus:

- `EditFocus::Timeline` is the default.
- `EditFocus::ClipEditor` is set when the clip editor is open and the user presses inside it.
- Clicking the timeline sets focus back to the timeline.

Clipboard, duplicate, and delete dispatch go to the focused editor. The rest of the global shortcuts (transport, save/open, undo/redo, loop, grouping, fit view) still work while the clip editor is open.

The clip editor's own actions run only when it has edit focus and no text field wants keyboard input.

## 8. Keyboard and pointer map

Shortcuts are disabled while a text field wants keyboard input.

| Input | Effect |
| --- | --- |
| `Space` | Play when stopped; Stop when playing |
| `Shift+Space` | Stop |
| `Cmd+S` / `Cmd+Shift+S` | Save / Save As |
| `Cmd+N` / `Cmd+O` | New / Open project |
| `Cmd+Z` / `Cmd+Shift+Z` | Undo / Redo |
| `Cmd+L` | Loop the time selection if one exists, else toggle loop |
| `Cmd+G` | Group selected tracks |
| `Cmd+0` | Fit arrangement |
| `Cmd+D` | Duplicate clips, time selection, or selected track, in that precedence |
| `Cmd+C` / `Cmd+X` / `Cmd+V` | Copy / cut / paste clips or time selection in the arranger |
| `Delete` / `Backspace` | Delete time selection, else devices, else clips, else non-master tracks |
| `Escape` | Close the focused clip editor, else clear selection |
| `Alt` while dragging | Bypass adaptive snap for clip move/trim, group move, bar move, and slice move/trim |
| `Shift` while selecting/creating | Bypass adaptive snap for lane time selection, ruler play-start, lane double-click, and clip-editor ruler/background |
| Click/drag ruler | Set play-start marker (and locate playhead while stopped) |
| Middle-drag | Pan horizontally; vertical component zooms horizontally around the pointer |
| Pinch / zoom gesture | Zoom around the pointer |
| Horizontal scroll | Pan horizontally |
| Drag audio file | Import and place on the arranger |

Ring/ruler details, track-drag rules, and clip gestures are in [`ARRANGER.md`](./ARRANGER.md) and [`EDITING.md`](./EDITING.md).

### Clipboard implementation

- The actual copied clips/slices are held in an app/editor-local clipboard.
- On copy/cut the UI also writes the marker text `notwig-clipboard` to the system clipboard. This makes the platform generate the paste event; it is not a cross-application audio/MIDI clipboard.
- Node selection wins over bar selection when choosing what to copy/cut/duplicate.

## 9. Audio file drop

The UI resolves an audio-drop target from the current row rectangles:

- Within 7 px of a row's top edge: insert a new track at that row's index.
- Within 7 px of a row's bottom edge: insert a new track after that row.
- Inside an audio track lane: target that track.
- Inside a non-audio lane: insert a new audio track after it.
- Below the last row: append a new audio track.

Multiple files are placed in order. The first uses the resolved target; further files create/insert successive tracks as needed.

The drag preview shows the target lane/new-track insertion line and a snapped ghost clip with the file name. Drop start is snapped with the adaptive grid. Clip duration is derived from decoded audio duration at the project tempo, with a minimum of 1/16 beat.

The preview is only shown while the pointer is over the arranger or the clip-editor content area.

Dropping onto the clip editor targets the current clip. The drop beat is clip-local and snapped with the clip-editor grid. The imported audio becomes a new audio slice at that beat; the clip is extended if the slice does not fit. Dropping onto the clip editor requires a saved bundle, like timeline drops.

## 10. Notifications and help

- Notifications are held in UI state with `Info`, `Warning`, and `Error` severities and are capped at 64 entries.
- The notifications window and help window are implemented, but the current shell has no visible control that sets their open flags. Treat them as unfinished surfaces.
- The help window's shortcut list is not authoritative; this document and the code are.

## 11. Theme and typography

Theme tokens live in `theme.rs`:

- Accent: warm amber.
- Playhead: red-orange.
- Panels: near-black greys.
- Audio clips: blue.
- MIDI clips: green.
- Selection/selection markers: warm yellow.
- Text: muted grey with white emphasis.

JetBrains Mono is installed as the first font for both proportional and monospace families; Lucide provides icons. Numeric readouts, labels, and dense UI text therefore use a monospace look throughout.

## 12. Invariants

1. The UI never mutates `ProjectDoc` directly.
2. Only one bottom editor is painted.
3. Opening/closing the clip editor preserves and restores device-panel visibility.
4. Playback visuals come from runtime state; semantic visuals come from the document.
5. Shortcuts do not fire while typing in a field.
6. Clipboard/duplicate/delete route to the focused editor.
7. Record controls are inert.
8. `MIX`, `EDIT`, Grid, Share, Monitor, and the track-row menu glyph are presentational placeholders in the current build.
9. Dropped audio is imported through the project bundle; unsaved projects prompt for Save As first.
