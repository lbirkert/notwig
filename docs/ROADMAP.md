# Roadmap

Notwig is a small, extensible DAW. This page records what the code supports today and what the core feature set should become. It is a status inventory, not a release schedule. Bitwig is the interaction reference, not a parity target.

## Status values

| Status | Meaning |
| --- | --- |
| ✅ Implemented | Covered by automated tests. |
| 🧪 Partial | Implemented in part, wired only on some paths, or missing automated coverage. Behavior may change. |
| 📋 Planned | Accepted for the core roadmap. Not implemented. |
| 🚫 Not Planned | Outside the core. Some entries are extension points. |

| Importance | Meaning |
| --- | --- |
| ⭐ | Low priority. |
| ⭐⭐ | Nice to have. |
| ⭐⭐⭐ | Useful, but not blocking. |
| ⭐⭐⭐⭐ | High priority for the core. |
| ⭐⭐⭐⭐⭐ | Essential. Blocks many other features. |

Move an item to ✅ Implemented only when its behavior has automated coverage at the lowest useful layer. Split a mixed row instead of hiding gaps in it. Importance is a rough product priority, not a schedule commitment.

## Scope

The target is a slimmed-down Bitwig-style DAW with a standard feature set: tracks, arranger, MIDI and audio editing, recording, automation, mixing, device chains, and plugin hosting. The device and editor systems are the main extension points.

These stay outside the core and become extensions: the clip launcher and scenes, The Grid and modular patching, Note FX, hybrid tracks, per-note modulation, and collaboration. Some are marked 🚫 Not Planned below.

## Where the code stands

The app is an arranger-first stereo DAW. It creates tracks, arranges MIDI and audio clips, edits audio slices, hosts VST3 and AU plugins, applies Tool, EQ, and Chain devices, mixes with volume, pan, mute, and solo, loops playback, and renders offline. A WASM extension host adds user-provided devices with DSP and custom panels.

The largest gaps are automation, recording, and warping.

## Working order

The current priorities, in order:

1. MIDI editing and a MIDI editor.
2. Audio editing and warping.
3. Plugin host depth.

After those, the core needs automation and recording. The real-time-safe engine boundary, recovery UI, settings persistence, plugin state, and hosted-plugin latency reporting are in place.

Dependencies worth planning around:

- A MIDI editor needs note editing commands, shared selection, and a shared clipboard.
- Warping needs a tempo map and an audio stretch engine.
- Recording needs input devices, monitoring, and punch handling.
- Automation needs stable parameter target IDs and an automation lane model.

## Project and application

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Directory project bundle | ✅ Implemented | ⭐⭐⭐⭐⭐ | `project.json` holds the embedded document. Save uses a temp file and atomic rename. |
| Schema versioning and migration | 🧪 Partial | ⭐⭐⭐ | Only the major version is checked. There is no migration framework. |
| Save and open | ✅ Implemented | ⭐⭐⭐⭐⭐ | Round-trip tests cover the manifest format. The loader also accepts legacy raw documents. |
| Dirty tracking | 🧪 Partial | ⭐⭐⭐ | Window title shows a marker. Opening or creating a project does not prompt about unsaved work. |
| Undo and redo | ✅ Implemented | ⭐⭐⭐⭐⭐ | Full-document snapshots, 200 entries. Transport and loop changes are excluded. Drag and parameter changes are not coalesced. |
| Autosave | 🧪 Partial | ⭐⭐⭐⭐ | Writes every 60 seconds when dirty. The startup recovery modal offers the latest autosave when a session lock exists. |
| Crash recovery | ✅ Implemented | ⭐⭐⭐⭐ | `session.lock` is created on open/save and removed on clean exit. A recovery modal loads or discards the latest autosave. Lock and workspace recovery paths are tested; the modal has no headless render test. |
| Audio asset import | ✅ Implemented | ⭐⭐⭐⭐⭐ | WAV, AIFF, and MP3 are covered. Files are resampled offline and stored as content-addressed float WAV. |
| FLAC and OGG import | 🧪 Partial | ⭐⭐⭐ | Routed through Symphonia but not covered by tests. |
| Asset relink | 🧪 Partial | ⭐⭐⭐ | Stores a path and decodes it. It does not copy the file into the bundle or verify the hash. |
| Asset garbage collection | 📋 Planned | ⭐⭐ |  |
| Recent projects | 📋 Planned | ⭐⭐ |  |
| Project templates | 📋 Planned | ⭐⭐ | New projects use one default template. There is no template picker or user template store. |
| Project rename UI | 📋 Planned | ⭐⭐ |  |
| Settings persistence | ✅ Implemented | ⭐⭐⭐⭐ | Shortcuts, audio driver, audio device, and block size persist in the per-user config. Missing drivers/devices fall back safely. Sample rate remains project-scoped. |
| Render mixdown | ✅ Implemented | ⭐⭐⭐⭐⭐ | Offline stereo render with WAV/AIFF output, 16/24-bit PCM or 32-bit float, optional dithering and normalization, plus stem export. |
| Export formats | ✅ Implemented | ⭐⭐⭐⭐ | WAV, AIFF, FLAC, MP3, and OGG Vorbis with bit depth where applicable, dithering, normalization, and stem export. |
| Render progress and cancel | 📋 Planned | ⭐⭐⭐ | GUI render blocks the UI thread today. |
| CLI commands | ✅ Implemented | ⭐⭐⭐ | `new`, `demo`, `render`, `info`, and `scan-plugins`. |
| Background engine builds | ✅ Implemented | ⭐⭐⭐⭐ | One build runs and one is queued. |

## Transport and timing

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Play, pause, stop | ✅ Implemented | ⭐⭐⭐⭐⭐ | Stop returns to loop start or zero and sends all-notes-off. |
| Global tempo | ✅ Implemented | ⭐⭐⭐⭐⭐ | One BPM value. Unsliced audio clip ranges rescale so their sample duration stays fixed. |
| Global time signature | ✅ Implemented | ⭐⭐⭐⭐ | One numerator and denominator. |
| Global loop | ✅ Implemented | ⭐⭐⭐⭐⭐ | Sample-accurate wrap, blocks split at the loop end. |
| Playhead locate | 🚫 Not Planned | ⭐⭐⭐⭐ | Ruler sets the play start. There is no scrub or seek gesture, and none is planned. |
| Metronome | ✅ Implemented | ⭐⭐⭐⭐ | Engine-generated click with a bar-start accent, toggled from the transport bar and stored in the project transport state. |
| Count-in | 📋 Planned | ⭐⭐⭐ |  |
| Tap tempo | 📋 Planned | ⭐⭐ |  |
| Tempo map and tempo automation | 📋 Planned | ⭐⭐⭐⭐ | Constant tempo only today. |
| Time signature changes | 📋 Planned | ⭐⭐⭐⭐ |  |
| Markers and cue points | 📋 Planned | ⭐⭐⭐⭐ |  |
| Follow playhead | 📋 Planned | ⭐⭐⭐ |  |
| Loop range handles | ✅ Implemented | ⭐⭐⭐ | Drag the loop rectangle edges on the ruler; time selection and shortcuts still set the range. |
| Punch in and out | 📋 Planned | ⭐⭐⭐⭐ |  |
| External clock and sync | 🚫 Not Planned | ⭐⭐ |  |

## Tracks, routing, and mixer

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Track kinds | ✅ Implemented | ⭐⭐⭐⭐⭐ | Audio, Instrument, Group, Bus FX, and Master. Only Audio and Instrument have lanes. |
| Create, delete, rename, color, and comment | ✅ Implemented | ⭐⭐⭐⭐⭐ |  |
| Track active state | 🧪 Partial | ⭐⭐⭐ | Stored, settable, and used to skip device processing. No dedicated test. |
| Track grouping and reordering | ✅ Implemented | ⭐⭐⭐⭐ | Group nesting works. Dragging into an empty group does not. |
| Duplicate track | ✅ Implemented | ⭐⭐⭐⭐ | Copies clips and devices. |
| Volume, pan, mute, solo | ✅ Implemented | ⭐⭐⭐⭐⭐ | Solo accounts for group ancestors and descendants. |
| Sends | 🚫 Not Planned | ⭐⭐⭐⭐⭐ | Engine routes pre and post fader, but there is no UI. Pre-fader is currently pre-FX too. Not planned. |
| Output routing | 🧪 Partial | ⭐⭐⭐⭐ | Engine routes track to track. No UI. |
| Bus and return tracks | 🚫 Not Planned | ⭐⭐⭐⭐⭐ | Bus FX exists as a track kind. There is no return or send-return pairing. Not planned. |
| Sidechain routing | 📋 Planned | ⭐⭐⭐⭐ | The routing graph can represent sidechain audio. No command or engine path creates it. |
| VCA groups | 📋 Planned | ⭐⭐⭐ |  |
| Track freeze and bounce in place | 📋 Planned | ⭐⭐⭐ |  |
| Track templates | 🚫 Not Planned | ⭐⭐ |  |
| Dedicated mixer view | 📋 Planned | ⭐⭐⭐⭐⭐ | Mixer controls live in track headers and the inspector. |
| Channel strip processing | 📋 Planned | ⭐⭐⭐⭐ | EQ and dynamics on every channel. |
| Peak and RMS meters | 🧪 Partial | ⭐⭐⭐⭐ | The engine computes both. The UI draws instantaneous peak only, with no hold or clip flag. |
| Record arm and monitoring | 📋 Planned | ⭐⭐⭐⭐⭐ | Buttons are painted but inert. |
| Hybrid tracks | 🚫 Not Planned | ⭐ | Bitwig-specific. Extension point. |
| Clip launcher and scenes | 🚫 Not Planned | ⭐ | Extension point. |

## Arranger and clip editing

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Clip create, move, trim, split, duplicate, delete, mute, name, color | ✅ Implemented | ⭐⭐⭐⭐⭐ |  |
| Overlap resolution | ✅ Implemented | ⭐⭐⭐⭐⭐ | Create, edit, move, and duplicate clear the destination range. |
| Multi-clip selection and group move | 🧪 Partial | ⭐⭐⭐⭐ | Additive selection works. Group move is horizontal only. |
| Marquee selection | 📋 Planned | ⭐⭐⭐ |  |
| Time selection create and move | ✅ Implemented | ⭐⭐⭐⭐⭐ |  |
| Time selection copy, cut, paste, duplicate, delete | ✅ Implemented | ⭐⭐⭐⭐⭐ | Shared edit algebra with the app. |
| Time selection edge trim | 📋 Planned | ⭐⭐⭐ | In and out markers are painted but not draggable. |
| Adaptive snap grid | ✅ Implemented | ⭐⭐⭐⭐ | Snap step changes with zoom. |
| User grid and snap settings | 📋 Planned | ⭐⭐⭐⭐ | No grid selector or snap toggle. |
| Zoom and pan | ✅ Implemented | ⭐⭐⭐⭐⭐ | Pinch zoom, middle-drag pan, and zoom to fit. |
| Audio drag and drop | ✅ Implemented | ⭐⭐⭐⭐⭐ | Drops onto a track, a new track, or the audio editor. |
| Clip gain | 🧪 Partial | ⭐⭐⭐⭐ | Core and engine support it. No editor. |
| Clip pan | 📋 Planned | ⭐⭐⭐ | Audio clip data has no pan field. |
| Fade handles and fade shapes | 📋 Planned | ⭐⭐⭐⭐ | Linear fades exist. No UI edits them and no other shape exists. |
| Crossfades | 📋 Planned | ⭐⭐⭐⭐ |  |
| Reverse | 📋 Planned | ⭐⭐ |  |
| Clip looping and repeat | 📋 Planned | ⭐⭐⭐⭐ |  |
| Ghost and linked clips | 📋 Planned | ⭐⭐⭐ |  |
| Warp markers and time stretch | 📋 Planned | ⭐⭐⭐⭐⭐ | Top audio priority. |
| Clip automation envelopes | 📋 Planned | ⭐⭐⭐⭐ |  |
| Ripple edit | 📋 Planned | ⭐⭐ |  |
| Take lanes | 📋 Planned | ⭐⭐⭐⭐ |  |
| Comping | 📋 Planned | ⭐⭐⭐⭐ | After basic recording. |
| Arranger sections | 📋 Planned | ⭐⭐⭐ |  |
| Clip launcher | 🚫 Not Planned | ⭐ | Extension point. |

## MIDI

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| MIDI note model | 🧪 Partial | ⭐⭐⭐⭐⭐ | Pitch, start, length, velocity, release velocity, and an opaque expression map. The expression map is unused. |
| Note commands | ✅ Implemented | ⭐⭐⭐⭐⭐ | `SetNote`, `CreateNote`, `SetNotes`, `DeleteNote`, and `DeleteNotes` validate MIDI clips, IDs, pitch/velocity, and clip bounds. Covered by apply-layer tests. |
| MIDI editor | ✅ Implemented | ⭐⭐⭐⭐⭐ | Shared clip chrome and beat strip, clickable piano keys with held note preview and key highlighting, notes outside the clip range, combined marquee/time/object selection, keyboard nudging, collision-aware resize/move, vertical velocity sliders, and headless interaction tests. |
| Note move, resize, and velocity editing | ✅ Implemented | ⭐⭐⭐⭐⭐ | Single and group move in time and pitch, edge resize, and velocity editing. |
| Multi-note selection and editing | ✅ Implemented | ⭐⭐⭐⭐ | Additive selection with group move, copy, cut, paste, duplicate, and delete. |
| Quantize, groove, swing, humanize | 📋 Planned | ⭐⭐⭐⭐ |  |
| Transpose and scale tools | 📋 Planned | ⭐⭐⭐ |  |
| MIDI clip looping | 📋 Planned | ⭐⭐⭐⭐ |  |
| MIDI CC, pitch bend, aftertouch, program change | 📋 Planned | ⭐⭐⭐⭐ | The data model has no event stream beyond notes. |
| MPE and per-note expression editing | 📋 Planned | ⭐⭐⭐ | The expression map is the starting point. |
| MIDI chase | 📋 Planned | ⭐⭐⭐ | Notes that start before the playhead do not sound today. |
| MIDI file import and export | 📋 Planned | ⭐⭐⭐⭐ |  |
| Drum editor and pad view | 📋 Planned | ⭐⭐⭐ |  |
| Step sequencer | 🚫 Not Planned | ⭐⭐ |  |
| MIDI monitor | 📋 Planned | ⭐⭐⭐ |  |
| MIDI input | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| MIDI output routing | 🚫 Not Planned | ⭐⭐⭐⭐ |  |
| MIDI and Note FX devices | 🚫 Not Planned | ⭐ | Extension point. |

## Audio

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Audio clip playback | ✅ Implemented | ⭐⭐⭐⭐⭐ | Sample-indexed source windows, gain, and fades. |
| Audio slices | ✅ Implemented | ⭐⭐⭐⭐ | Per-slice source window, gain, and optional asset. Events may extend beyond the clip container; playback clips them to the container. |
| Audio editor | ✅ Implemented | ⭐⭐⭐⭐ | Slice selection, move, trim, copy, cut, paste, duplicate, delete, and time selection. |
| Audio trim | ✅ Implemented | ⭐⭐⭐⭐⭐ | Clip range edits trim the source window. |
| Linear fades | 🧪 Partial | ⭐⭐⭐⭐ | Stored and rendered. No editor and no fade-shape choice. |
| Audio clip gain | 🧪 Partial | ⭐⭐⭐⭐ | Stored and rendered. No editor. |
| Clip pan | 📋 Planned | ⭐⭐⭐ |  |
| Fade shapes | 📋 Planned | ⭐⭐⭐ |  |
| Reverse | 📋 Planned | ⭐⭐ |  |
| Time stretch and warping | 📋 Planned | ⭐⭐⭐⭐⭐ | Top audio priority. Clips are sample-locked to the current tempo today. |
| Warp markers and transient detection | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Pitch shift | 📋 Planned | ⭐⭐⭐ |  |
| Audio quantize and groove | 📋 Planned | ⭐⭐⭐ |  |
| Audio recording | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Loop and punch recording | 📋 Planned | ⭐⭐⭐⭐ |  |
| Comping and takes | 📋 Planned | ⭐⭐⭐⭐ | After basic recording. |
| Input monitoring | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Input routing | 📋 Planned | ⭐⭐⭐⭐⭐ | Input fields exist but are always empty. |
| Mono and stereo track formats | 📋 Planned | ⭐⭐⭐⭐ | Engine and assets are stereo today. |
| Surround and multi-channel | 🚫 Not Planned | ⭐ |  |
| Sample-rate conversion on rate change | 📋 Planned | ⭐⭐⭐ | The app warns that existing assets are not resampled. |
| Waveform peak cache | 🧪 Partial | ⭐⭐⭐ | Peaks are recomputed in memory. The bundle `peaks/` directory is unused. |

## Automation and modulation

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Automation lanes | 📋 Planned | ⭐⭐⭐⭐⭐ | Automation editor kinds exist as tags only. |
| Automation editing tools | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Automation modes | 📋 Planned | ⭐⭐⭐⭐ | Read, touch, latch, write. |
| Parameter modulation | 🚫 Not Planned | ⭐⭐⭐⭐ | Modulation and macro flags exist as reserved metadata. Not planned for now. |
| Macro controls | 🚫 Not Planned | ⭐⭐⭐⭐ | Not planned for now. |
| MIDI learn | 🚫 Not Planned | ⭐⭐⭐ | Not planned for now. |
| Control surface support | 🚫 Not Planned | ⭐⭐⭐ | Not planned for now. |
| Sample-accurate parameter ramps | 🚫 Not Planned | ⭐⭐⭐⭐ | Parameter changes apply at block boundaries. `RampSpec` is carried and ignored. Not planned for now. |
| Modulator devices | 🚫 Not Planned | ⭐⭐⭐ | Basic LFO and envelope modulators. Not The Grid. Not planned for now. |
| The Grid and modular patching | 🚫 Not Planned | ⭐ | Extension point. |
| Per-note modulation | 🚫 Not Planned | ⭐ | Bitwig-specific. Extension point. |

## Devices

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Device chains | 🧪 Partial | ⭐⭐⭐⭐⭐ | Insert, move, remove, bypass, and parameter commands exist. The UI cannot reorder devices. |
| Nested Chain container | ✅ Implemented | ⭐⭐⭐⭐ | Recursive device chains with dry/wet. |
| Tool | ✅ Implemented | ⭐⭐⭐⭐ | Input gain, output gain, pan, width, polarity, and dry/wet. |
| EQ | ✅ Implemented | ⭐⭐⭐⭐⭐ | Four bands with bell, shelves, high pass, and low pass, plus shift and output gain. |
| Chain | ✅ Implemented | ⭐⭐⭐⭐ | Input gain, output gain, and dry/wet container. |
| Stock instruments | 📋 Planned | ⭐⭐ | Start with one simple synth and one sampler. |
| Stock effects | 📋 Planned | ⭐⭐⭐ | Delay, reverb, compressor, limiter, filter, saturation, and chorus. |
| Device presets | 📋 Planned | ⭐⭐⭐⭐ |  |
| Device A/B compare | 🚫 Not Planned | ⭐⭐⭐ | Disable the plugin to compare A/B. |
| Device drag reorder | 📋 Planned | ⭐⭐⭐⭐ | Insert points exist, drag does not. |
| Device latency and PDC | ✅ Implemented | ⭐⭐⭐⭐⭐ | Sends, nested groups, bypassed devices, inactive tracks, track routing, and hosted VST3/AU plugin latency are compensated. |
| Device browser | ✅ Implemented | ⭐⭐⭐⭐ | Search, keyboard navigation, stock, extension, and plugin entries. |
| Extension devices | 🧪 Partial | ⭐⭐⭐⭐ | WASM devices provide metadata, DSP, and custom panels; `notwig-ext-devices` ships a limiter example. No note-event forwarding or device state blobs yet. |

## Plugins

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| VST3 hosting | 🧪 Partial | ⭐⭐⭐⭐⭐ | In-process load and process. No integration tests; the plugin API cannot report latency yet. |
| AU hosting | 🧪 Partial | ⭐⭐⭐⭐ | In-process on macOS. No integration tests. |
| CLAP hosting | 📋 Planned | ⭐⭐⭐⭐ |  |
| VST2 hosting | 📋 Planned | ⭐ |  |
| Plugin scan, cache, and blacklist | ✅ Implemented | ⭐⭐⭐⭐⭐ | Scan runs in a child process. Cache is JSON with failure counts. |
| Native plugin editor | 🧪 Partial | ⭐⭐⭐⭐ | macOS only, uses a separate plugin instance, and polls parameters. |
| Plugin parameter UI | 🧪 Partial | ⭐⭐⭐⭐ | Generic parameter list. UUID to `u32` conversion truncates plugin parameter IDs. |
| Plugin state save and restore | ✅ Implemented | ⭐⭐⭐⭐⭐ | Native editor state is saved on close/exit, stored under `state/` in the bundle, and restored when plugin instances are rebuilt. |
| Plugin MIDI output | 📋 Planned | ⭐⭐⭐ | Output events are collected and discarded. |
| Sidechain and multi-out buses | 📋 Planned | ⭐⭐⭐⭐ |  |
| Plugin latency compensation | ✅ Implemented | ⭐⭐⭐⭐⭐ | The plan and engine compensate non-zero device latency, including latency queried from vendored VST3/AU wrappers. |
| Out-of-process hosting | 📋 Planned | ⭐⭐⭐⭐ | Crash isolation and sandboxing. |
| Plugin crash recovery | 📋 Planned | ⭐⭐⭐⭐ |  |
| Plugin preset management | 📋 Planned | ⭐⭐⭐ |  |

## Extensions

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| WASM extension host | ✅ Implemented | ⭐⭐⭐⭐ | `wasmtime` host, core wasm ABI, user-directory discovery, compiled module cache. |
| Extension device DSP | 🧪 Partial | ⭐⭐⭐⭐ | Block `f32` processing, parameters, dynamic latency. Note events and state blobs are not forwarded yet. |
| Extension device panels | ✅ Implemented | ⭐⭐⭐⭐ | Render-neutral draw commands plus native knob/toggle commands, with generic fallback. |
| Extension trust levels | ✅ Implemented | ⭐⭐⭐⭐ | Trusted, Limited (fuel), and DrawOnly. Persisted per extension. |
| Extension registry and browser | ✅ Implemented | ⭐⭐⭐⭐ | Settings explorer has All/Available/Installed filters, icons/descriptions, context actions, manage-index window, and configurable index sources. |
| Extension install and update | ✅ Implemented | ⭐⭐⭐⭐ | Install from index URLs/paths or local ZIPs, confirm reinstall, uninstall cleans files and settings. |
| General drawing hooks | 📋 Planned | ⭐⭐⭐ | Arranger clips, track headers, lanes, and editors on the shared draw IR. |
| Official extension index | 🧪 Partial | ⭐⭐⭐ | `docs/extensions.txt` builds `docs/extensions.json`; CI packages the stock extension and publishes via GitHub Pages. |

## UI and workflow

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Transport bar | ✅ Implemented | ⭐⭐⭐⭐⭐ | Play, stop, loop, tempo, time signature, position, master meter, CPU load, settings. |
| Arranger | ✅ Implemented | ⭐⭐⭐⭐⭐ | Tracks, clips, time selection, loop, grid, playhead, drop targets. |
| Contextual inspector | 🧪 Partial | ⭐⭐⭐⭐ | Some audio fade and gain fields are read-only. |
| Device panel | ✅ Implemented | ⭐⭐⭐⭐⭐ | Chain cards, bypass, nesting, EQ editor, generic parameter knobs. |
| Audio editor | ✅ Implemented | ⭐⭐⭐⭐ | Also see the Audio table. |
| MIDI editor | ✅ Implemented | ⭐⭐⭐⭐⭐ | Opens for MIDI clips from the arranger and the clip editor toggle. |
| Mixer view | 🚫 Not Planned | ⭐⭐ | Not required in core. Mixing works through track headers and the inspector. |
| Automation editor | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Project and asset browser | 📋 Planned | ⭐⭐⭐ | Only the device browser exists. |
| Settings | 🧪 Partial | ⭐⭐⭐⭐ | Audio and Shortcuts pages. General and Appearance are placeholders. |
| DSP load window | ✅ Implemented | ⭐⭐ | Shows callback history, average and max load, deadline, jitter, and output latency. The window render is tested. |
| Render mixdown dialog | ✅ Implemented | ⭐⭐⭐⭐ | Choose format, bit depth, dithering, normalization, stems, range, and output path. |
| Notifications and help windows | 🧪 Partial | ⭐⭐ | Implemented but unreachable from the shell. |
| Keyboard shortcuts | ✅ Implemented | ⭐⭐⭐⭐ | Configurable in Settings -> Shortcuts. Commands can have multiple bindings; bindings persist per user. |
| Tool palette | 📋 Planned | ⭐⭐⭐ | All gestures are implicit today. |
| Shared clipboard | ✅ Implemented | ⭐⭐⭐⭐ | The arranger and audio editor share the workspace clipboard; cross-domain paste is rejected. |
| Undo history panel | 📋 Planned | ⭐⭐ |  |

## Platform and runtime

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| macOS | 🧪 Partial | ⭐⭐⭐⭐⭐ | Primary development platform. No CI matrix. |
| Windows and Linux | 📋 Planned | ⭐⭐⭐ | Audio backend and platform adapters needed. |
| Real-time-safe audio path | 🧪 Partial | ⭐⭐⭐⭐⭐ | The callback uses `try_lock` and installs prepared `EngineData` without waiting on the UI. Allocator and plugin-callback instrumentation is still missing. |
| Lock-free engine command queue | 🚫 Not Planned | ⭐⭐⭐⭐⭐ | Not needed: the app swaps prepared `EngineData` and the callback installs it without a command queue. |
| Sample-accurate scheduling | ✅ Implemented | ⭐⭐⭐⭐⭐ | Notes, audio clips, and loop boundaries. Parameter changes are excluded. |
| Offline and realtime parity | ✅ Implemented | ⭐⭐⭐⭐⭐ | Block-size invariance is covered. |
| Parallel track processing | 📋 Planned | ⭐⭐⭐ |  |
| DSP load reporting | 🧪 Partial | ⭐⭐⭐ | Callback load and jitter are measured. CPU load is not reported by the engine. |
| Audio device hot-plug | 📋 Planned | ⭐⭐⭐ |  |
| Crash reporting | 📋 Planned | ⭐⭐⭐ |  |

## Extension points

These are outside the core. The device and editor systems should leave room for them, but no core work is scheduled.

- Clip launcher, scenes, and follow actions.
- The Grid and modular patching.
- Note FX, operators, and hybrid tracks.
- Per-note modulation and advanced MPE modulation.
- Collaboration and project sharing.
- Scripting and a device marketplace.
- Additional plugin formats beyond the core set.

## Known blockers

| Blocker | Importance | Why it matters |
| --- | --- | --- |
| Audio clips are sample-locked | ⭐⭐⭐⭐⭐ | Tempo changes can invalidate clips until warping exists. |

## Updating this page

When a feature changes status, edit the row and its notes. Move an item to ✅ Implemented only after the test exists. When scope is dropped, move the item to 🚫 Not Planned or to the extension points list and record the reason in the relevant design document. Test coverage is tracked separately in [`TESTING.md`](TESTING.md).
