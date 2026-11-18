# Roadmap

Notwig is deliberately feature-light. This page records what exists today and
what is planned or deferred. It is directional, not a promise of dates or
feature parity with any other DAW.

## Available now

### Projects and persistence

- Directory-based `*.notwig` project bundles with a versioned JSON manifest.
- New project templates at 44.1, 48, and 96 kHz.
- Open, Save, and Save As.
- Autosave snapshots and recovery detection.
- Missing-asset and validation issues reported on load.
- Undo/redo with 200 entries; mixer and parameter changes use cheap
  non-structural undo.
- Managed audio assets, relinking, and import.
- Offline mixdown to 32-bit float WAV, including range rendering.

### Arranger

- Linear Arranger with a bar/beat ruler, grid, playhead, and play-start marker.
- Audio, instrument, group, bus FX, and master tracks.
- Track rename, color, comment, active state, mute, solo, volume, and pan.
- Track grouping and duplication.
- MIDI and audio clips.
- Clip create, move, trim, split, duplicate, mute, and delete.
- Manual clip start/end bounds; events can sit outside the visible clip bounds.
- Object selection and time-range selection.
- Copy, cut, paste, delete, duplicate, and move for objects and time ranges.
- 1/16 snap by default; hold Alt while dragging to bypass it.
- Audio drag-and-drop onto tracks, new tracks, or the audio clip editor.
- Pinned master track and fit-arrangement navigation.

### Audio clip editor

- Bottom panel for audio clips with a waveform view and event (slice) editing.
- Move, trim, split, duplicate, and delete audio events.
- Time-range selection inside the clip with copy/cut/paste/delete/duplicate.
- Add audio events by dropping files into the clip editor.
- Double-click an audio track to create a silent audio clip.
- Shared editing algebra and undo behavior with the Arranger.

### Devices and information

- Device chains with stock EQ, Tool, and Chain devices.
- Device browser, parameter controls, bypass, removal, and plugin reload.
- EQ curve display for the stock EQ; meters in the track/info views.
- Native plugin editor windows.
- Info panel for tracks, devices, MIDI clips, audio clips, files, and time
  selections.

### Transport and audio

- Play, pause, stop, loop, tempo, time signature, play-start, and playhead.
- Audio driver, device, sample rate, and block-size settings.
- DSP load analysis window with the last 200 callback loads, average/max load,
  deadline, period jitter, and output latency.
- Engine builds happen off the UI thread; structural changes swap in a rebuilt
  plan.

### Plugins

- In-process VST3 and AU hosting on macOS through `truce-rack`.
- Background plugin discovery with a persistent cache.
- Instrument and audio-effect plugins.
- Plugin parameters, state, and native editor windows.

### Keyboard and mouse essentials

- `Space`: play/stop; `Shift+Space`: stop.
- `Cmd+S` / `Cmd+Shift+S`: save / save as.
- `Cmd+N` / `Cmd+O`: new / open project.
- `Cmd+D`: duplicate; `Cmd+C` / `Cmd+X` / `Cmd+V`: copy / cut / paste.
- `Cmd+Z` / `Cmd+Shift+Z`: undo / redo.
- `Cmd+L`: loop the selected time range.
- `Delete`: delete selection, clips, or time range.
- `Cmd+0`: fit arrangement.
- Click the ruler to set play start; drag clip bodies/edges to move or trim;
  right-click a clip for mute/duplicate/delete.

## Candidate next milestones

These are the most likely next steps. Order and scope may change.

- Piano-roll MIDI note editing.
- Automation editing for device and mixer parameters.
- Audio and MIDI recording, live input, and monitoring.
- Clip Launcher and scenes.
- Audio warping and time-stretching.
- Tempo/time-signature automation.
- Modulation and macro runtime.
- Metronome and count-in.
- Stock instruments.
- Windows and Linux support.
- Out-of-process plugin hosting and collaboration.

## Deferred or intentionally limited

- No feature-parity goal with Bitwig or other commercial DAWs.
- VST2 is represented in the plugin model but is not currently hosted.
- MIDI clips currently support clip-level editing only; there is no note editor.
- Audio editing is non-destructive slicing/trimming with gain; no warping or
  stretching.
- macOS is the only actively supported platform.

## Updating this page

When a feature lands, move it from a candidate milestone into "Available now".
When scope is intentionally dropped, move it to "Deferred" or remove it and
explain the decision in the relevant design document.
