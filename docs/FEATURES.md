# Features

This is the current user-facing feature set. "Implemented" means it exists in
the app today; "limited" means it works but intentionally covers a small subset;
"deferred" means it is reserved for a later version.

## Transport and top bar

**Implemented**

- Play, pause, and stop.
- Play-start marker separate from the playhead: click the ruler to set where
  playback starts; the playhead can move independently while playing.
- Loop toggle and loop range.
- Tempo and time signature controls.
- CPU/DSP component in the engine strip. Click the icon or bar to open the DSP
  load analysis window.
- Settings entry points: top-bar gear, `FILE -> Audio Settings...`, and `Cmd+,`.

**Code**: `notwig-ui/src/shell.rs`, `notwig-ui/src/dsp.rs`,
`notwig-app/src/gui.rs`.

## Arranger

**Implemented**

- Linear timeline with bar/beat ruler, grid, playhead, and play-start marker.
- Audio, instrument, group, bus FX, and master tracks.
- Pinned master track below the scrolling track list.
- Track rename, color, comment, active state, mute, solo, volume, and pan.
- Track grouping and duplication.
- MIDI and audio clips.
- Clip creation on empty lanes, clip move, trim, split, duplicate, mute, and
  delete.
- Manual clip start/end bounds; events can sit outside the visible clip bounds.
- Object selection and time-range selection.
- Copy, cut, paste, delete, duplicate, and move for objects and time ranges.
- 1/16 snap by default; hold Alt to bypass snapping.
- Fit arrangement (`Cmd+0`).
- Audio file drag-and-drop onto a track, a new track, or the audio clip editor.

**Limited**

- MIDI clips are edited at the clip level only; there is no piano-roll note editor.
- Audio editing is non-destructive slicing/trimming with gain; no warping,
  stretching, or time-stretching.

**Code**: `notwig-ui/src/arranger.rs`, `notwig-ui/src/timeline/`,
`notwig-app/src/workspace/mod.rs`, `notwig-app/src/edit_lowering.rs`.

## Audio clip editor

**Implemented**

- Bottom panel opened from the footer or by opening an audio clip.
- Waveform rendering that maps the full event to the visible clip range.
- Audio events (slices) with move, trim, split, duplicate, and delete.
- Time-range selection inside the clip with copy/cut/paste/delete/duplicate.
- Drop audio files into the clip editor to add events.
- Double-click an audio track to create a silent audio clip.
- Shared edit algebra and undo behavior with the arranger.

**Code**: `notwig-ui/src/panel/clip_editor.rs`, `notwig-ui/src/timeline/`,
`notwig-core/src/edit/`, `notwig-app/src/edit_lowering.rs`.

## Info panel

**Implemented**

- Left-side panel toggled from the footer.
- Track view: name, kind, color, comment, mixer, routing, clips, and devices.
- Device view: device identity, parameters, and chain context.
- MIDI clip view: clip range, content length, and note summary.
- Audio clip view: asset, ranges, gain, fades, and event/slice details.
- Files view: project bundle path and asset information.
- Time selection view: range and affected tracks.

**Code**: `notwig-ui/src/info/`.

## Device panel and browser

**Implemented**

- Bottom device editor panel.
- Device chains per track.
- Stock EQ, Tool, and Chain devices.
- Parameter controls (knobs, faders, draggable numbers) and bypass.
- Generic parameter view for hosted plugins.
- EQ curve display for the stock EQ.
- Add Device browser for stock devices and discovered plugins.
- Remove devices and open native plugin editor windows.

**Code**: `notwig-ui/src/panel/device.rs`, `notwig-ui/src/device/`,
`notwig-dsp/src/stock.rs`.

## Plugins

**Implemented**

- In-process VST3 and AU hosting on macOS through `truce-rack`.
- Background plugin discovery in a separate scanner process with a persistent
  cache.
- Instrument and audio-effect plugins.
- Plugin descriptors exposed to the UI; plugin parameters, state, and native
  editor windows.
- Plugin folders are filtered to the current project's plugin types.

**Limited**

- VST2 exists in the format model but is not currently hosted.
- No out-of-process plugin sandboxing.

**Code**: `notwig-plugins/`, `notwig-app/src/plugins.rs`,
`notwig-ui/src/device/`.

## Project lifecycle

**Implemented**

- Directory-based `*.notwig` project bundles.
- New project templates at 44.1, 48, and 96 kHz.
- Open, Save, and Save As.
- Versioned manifest with schema-major validation.
- Missing-asset and validation issues reported on load.
- Autosave snapshots and recovery detection.
- Undo/redo with 200 entries.
- Structural changes trigger an engine rebuild; mixer/parameter changes use a
  lightweight engine synchronization path.
- Audio import, relink, and managed asset copies.
- Offline mixdown to 32-bit float WAV.

**Code**: `notwig-project/`, `notwig-app/src/workspace/`,
`notwig-app/src/history.rs`, `notwig-app/src/gui.rs`.

## Settings

**Implemented**

- Audio page: driver, output device, sample rate, block size, latency estimate,
  and apply/revert.
- Audio settings are applied by restarting the CPAL stream; the project sample
  rate can also be changed and is undoable.

**Limited**

- General and Appearance categories are placeholders.

**Code**: `notwig-ui/src/settings.rs`, `notwig-app/src/audio.rs`.

## Render mixdown

**Implemented**

- Render window from `FILE -> Render Mixdown...`.
- Start/end beats with a copy-loop-range shortcut.
- 32-bit float WAV output.
- Offline rendering uses a freshly built engine and the same plan/schedule as
  realtime playback.

**Code**: `notwig-ui/src/render.rs`, `notwig-app/src/workspace/mod.rs`.

## DSP analysis

**Implemented**

- Popup opened by clicking the CPU/DSP component.
- Bar chart of the last 200 DSP callback loads, oldest to newest left-to-right,
  newest at the right.
- Stats: block size, sample rate, deadline, average/max load in milliseconds,
  period jitter, input/output latency, and timing source.
- Usage is measured as `processing_time / (block_size / sample_rate)`.

**Code**: `crates/notwig-ui/src/dsp.rs`, `crates/notwig-app/src/audio.rs`,
`crates/notwig-ui/src/runtime.rs`.

## Keyboard and mouse shortcuts

| Shortcut | Action |
|---|---|
| `Space` | Play / stop (returns to play start) |
| `Shift+Space` | Stop |
| `Cmd+S` / `Cmd+Shift+S` | Save / Save As |
| `Cmd+N` / `Cmd+O` | New / open project |
| `Cmd+D` | Duplicate selected clips or time range |
| `Cmd+C` / `Cmd+X` / `Cmd+V` | Copy / cut / paste |
| `Cmd+Z` / `Cmd+Shift+Z` | Undo / redo |
| `Cmd+L` | Loop the selected time range |
| `Delete` | Delete selection, clips, or time range |
| `Cmd+0` | Fit arrangement |
| `Alt` while dragging | Bypass 1/16 snapping |
| Double-click empty lane | Create a 4-beat MIDI clip |
| Drag clip body / edges | Move or trim |
| Right-click clip | Mute, duplicate, delete |
| Click ruler | Set play start |

## Not implemented yet

- Audio/MIDI recording, live input, and monitoring.
- Piano-roll MIDI note editing and automation editing.
- Clip Launcher/scenes and comping/takes.
- Audio warping, stretching, and time-stretching.
- Tempo/time-signature automation.
- Modulation and macro runtime.
- Metronome and count-in.
- Stock instruments.
- Windows/Linux builds.
- Collaboration and out-of-process plugin hosting.
