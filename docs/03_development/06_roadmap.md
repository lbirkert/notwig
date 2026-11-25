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

The app is an arranger-first stereo DAW. It creates tracks, arranges MIDI and audio clips, edits audio slices, hosts VST3, VST2, CLAP, LV2, and AU (v2/v3) plugins, applies Tool, EQ, and Chain devices, mixes with volume, pan, mute, and solo, loops playback, and renders offline. An extension host adds user-provided WASM or native devices with DSP and custom panels.

The largest gaps are automation, warping, and advanced recording workflows.

## Working order

The current priorities, in order:

1. Audio editing and warping.
2. Advanced recording: MIDI, takes, loop, and punch.
3. Automation lanes and editing.
4. Plugin host depth.

The MIDI editor, real-time-safe engine boundary, recovery UI, settings
persistence, plugin state, hosted-plugin latency reporting, and linear audio
recording with per-track input are in place. Warping and automation remain the
largest missing core workflows.

Dependencies worth planning around:

- Warping needs a tempo map and an audio stretch engine.
- Advanced recording needs MIDI input, take lanes, and punch handling.
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
| Audio asset import | ✅ Implemented | ⭐⭐⭐⭐⭐ | WAV, AIFF, and MP3 import paths are covered by tests. Files are resampled offline and stored as content-addressed float WAV. |
| FLAC and OGG import | 🧪 Partial | ⭐⭐⭐ | Routed through Symphonia; probe and export round-trips are covered, but import-decode tests are still missing. |
| Asset relink | 🧪 Partial | ⭐⭐⭐ | Stores a path and decodes it. It does not copy the file into the bundle or verify the hash. |
| Asset garbage collection | 📋 Planned | ⭐⭐ |  |
| Recent projects | ✅ Implemented | ⭐⭐ | FILE menu submenu backed by the native registry and browser local storage. |
| Project templates | 📋 Planned | ⭐⭐ | New projects use one default template. There is no template picker or user template store. |
| Project rename UI | 📋 Planned | ⭐⭐ |  |
| Settings persistence | ✅ Implemented | ⭐⭐⭐⭐ | Shortcuts, audio driver, audio device, and block size persist in the per-user config. Missing drivers/devices fall back safely. Sample rate remains project-scoped. |
| Render mixdown | ✅ Implemented | ⭐⭐⭐⭐⭐ | Offline stereo render with WAV/AIFF output, 16/24-bit PCM or 32-bit float, optional dithering and normalization, plus stem export. |
| Export formats | ✅ Implemented | ⭐⭐⭐⭐ | WAV, AIFF, FLAC, MP3, and OGG Vorbis with bit depth where applicable, dithering, normalization, and stem export. |
| Render progress and cancel | 📋 Planned | ⭐⭐⭐ | GUI render blocks the UI thread today. |
| CLI commands | ✅ Implemented | ⭐⭐⭐ | `new`, `demo`, `render`, `info`, `scan-plugins`, and `loopback`. |
| Background engine builds | ✅ Implemented | ⭐⭐⭐⭐ | One build runs and one is queued. |

## Transport and timing

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Play, pause, stop | ✅ Implemented | ⭐⭐⭐⭐⭐ | Stop returns to the last play start and sends all-notes-off. |
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
| VCA groups | 🚫 Not Planned | ⭐⭐⭐ |  |
| Track freeze and bounce in place | 📋 Planned | ⭐⭐⭐ |  |
| Track templates | 🚫 Not Planned | ⭐⭐ |  |
| Dedicated mixer view | 🚫 Not Planned | ⭐⭐⭐⭐⭐ | Mixer controls live in track headers and the inspector. |
| Channel strip processing | 📋 Planned | ⭐⭐⭐⭐ | EQ and dynamics on every channel. |
| Peak and RMS meters | 🧪 Partial | ⭐⭐⭐⭐ | The engine computes peak and RMS. The UI draws peak with hold and click-to-reset; RMS display and a clip flag are missing. |
| Record arm and monitoring | ✅ Implemented | ⭐⭐⭐⭐⭐ | Per-track audio input, arm state, and chain monitoring. Monitoring is tied to arm. |
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
| Time selection edge trim | ✅ Implemented | ⭐⭐⭐ | Drag either selection edge to extend or shorten the time range. |
| Adaptive snap grid | ✅ Implemented | ⭐⭐⭐⭐ | Uses a 40 px minimum spacing and follows zoom. |
| User grid and snap settings | ✅ Implemented | ⭐⭐⭐⭐ | Global snap toggle plus adaptive/manual levels from 2/1 to 1/65536. |
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
| Comping | 🚫 Not Planned | ⭐⭐⭐⭐ | After basic recording. |
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
| Audio editor | ✅ Implemented | ⭐⭐⭐⭐ | Host-owned Audio Events/Gain sub-editor selector, slice selection, move, trim, copy, cut, paste, duplicate, delete, and time selection. |
| Audio trim | ✅ Implemented | ⭐⭐⭐⭐⭐ | Clip range edits trim the source window. |
| Linear fades | 🧪 Partial | ⭐⭐⭐⭐ | Stored and rendered. No editor and no fade-shape choice. |
| Audio clip gain | ✅ Implemented | ⭐⭐⭐⭐ | Stored and rendered. The Gain sub-editor edits clip gain or per-event gain. |
| Clip pan | 📋 Planned | ⭐⭐⭐ |  |
| Fade shapes | 📋 Planned | ⭐⭐⭐ |  |
| Reverse | 📋 Planned | ⭐⭐ |  |
| Time stretch and warping | 📋 Planned | ⭐⭐⭐⭐⭐ | Top audio priority. Clips are sample-locked to the current tempo today. |
| Warp markers and transient detection | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Pitch shift | 📋 Planned | ⭐⭐⭐ |  |
| Audio quantize and groove | 📋 Planned | ⭐⭐⭐ |  |
| Audio recording | 🧪 Partial | ⭐⭐⭐⭐⭐ | Toggleable linear record mode with live growing clips and waveforms, mono or stereo input capture, destination replacement, and browser microphone capture. Loop/punch recording and takes are planned. |
| Loop and punch recording | 📋 Planned | ⭐⭐⭐⭐ | Linear recording is implemented first. |
| Take lanes | 📋 Planned | ⭐⭐⭐⭐ | After basic recording. |
| Comping | 🚫 Not Planned | ⭐⭐⭐⭐ | After basic recording. |
| Input monitoring | 🧪 Partial | ⭐⭐⭐⭐⭐ | Armed tracks support Disabled, Enabled through-chain, and Low Latency monitoring through a separate track-chain instance on a 16-frame device buffer while the project renders in larger chunks. Hardware calibration is still planned. |
| Input routing | 🧪 Partial | ⭐⭐⭐⭐⭐ | Single selected device with named mono and stereo inputs using independent left/right channels, plus named output and master-output selection. Multi-device recording is intentionally unsupported. |
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
| Device chains | ✅ Implemented | ⭐⭐⭐⭐⭐ | Insert, move, remove, bypass, copy/cut/paste, activate/deactivate, and parameter commands work in nested chains. |
| Nested Chain container | ✅ Implemented | ⭐⭐⭐⭐ | Recursive device chains with dry/wet. |
| Tool | ✅ Implemented | ⭐⭐⭐⭐ | Input gain, output gain, pan, width, polarity, and dry/wet. |
| EQ | ✅ Implemented | ⭐⭐⭐⭐⭐ | Four bands with bell, shelves, high pass, and low pass, plus shift and output gain. |
| Chain | ✅ Implemented | ⭐⭐⭐⭐ | Input gain, output gain, and dry/wet container. |
| Extension container chains | ✅ Implemented | ⭐⭐⭐⭐ | Extension `DeviceKind::Container` devices get a host-owned serial child chain via `DeviceSpec::container`, including a wet FX chain. The bundled Reverb uses it. Parallel and sidechain routing remain unsupported. |
| Stock instruments | 📋 Planned | ⭐⭐ | Start with one simple synth and one sampler. A WASM synth example ships in `notwig-ext-devices`, but no stock instrument exists yet. |
| Stock effects | 📋 Planned | ⭐⭐⭐ | Core filter and saturation. Compressor, clipper, delay, chorus, transient, limiter, gate, and reverb ship in the bundled extension pack. |
| Device presets | 📋 Planned | ⭐⭐⭐⭐ |  |
| Device A/B compare | 🚫 Not Planned | ⭐⭐⭐ | Disable the plugin to compare A/B. |
| Device drag reorder | ✅ Implemented | ⭐⭐⭐⭐ | Drag handles reorder within and across nested chains; dropping on an arranger lane or track header appends to its main chain. |
| Device activation | ✅ Implemented | ⭐⭐⭐⭐ | Alt+A toggles selected devices; deactivation unloads processors while keeping document state. Enable/disable keeps plugins loaded. |
| Device latency and PDC | ✅ Implemented | ⭐⭐⭐⭐⭐ | Sends, nested groups, bypassed devices, inactive tracks, track routing, and hosted VST3/VST2/AU plugin latency are compensated. Bypass keeps planned latency stable without rebuilding. |
| Device browser | ✅ Implemented | ⭐⭐⭐⭐ | Search, keyboard navigation, stock, extension, and plugin entries. |
| Extension devices | 🧪 Partial | ⭐⭐⭐⭐ | WASM and native devices provide metadata, DSP, custom panels, note events, and host-owned container chains; `notwig-ext-devices` ships a compressor, clipper, delay, chorus, transient control, limiter, gate, Room/Hall/Plate reverb with a wet FX chain, and a polyphonic synth. Device state blobs are still deferred. |

## Plugins

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| VST3 hosting | 🧪 Partial | ⭐⭐⭐⭐⭐ | In-process load and process. Manual truce fixture integration tests cover scan/load/params/state/latency; no CI plugin bundle yet. |
| VST2 hosting | 🧪 Partial | ⭐ | Enabled on native macOS builds through the vendored wrapper. Manual truce fixture integration tests cover the same host path. |
| CLAP hosting | 🧪 Partial | ⭐⭐⭐⭐ | In-process scan/load/process/state/GUI through the vendored wrapper. Manual truce fixture integration tests cover scan/load/params/state/editor. |
| LV2 hosting | 🧪 Partial | ⭐⭐⭐ | In-process through lilv; requires the system `lilv-0` library. Native-class UIs only. Manual fixture covers discovery/load/process and asserts the remaining parameter/state gaps. |
| AU v2 hosting | 🧪 Partial | ⭐⭐⭐⭐ | In-process on macOS. Manual fixture tests exist and require installing the AU component. |
| AU v3 hosting | 🧪 Partial | ⭐⭐⭐ | Registry scan and load on macOS through the vendored AU wrapper. Async-only v3 instantiation is not yet exercised. |
| Plugin scan, cache, and blacklist | ✅ Implemented | ⭐⭐⭐⭐⭐ | Scan runs in a child process. Cache is JSON with failure counts. |
| Native plugin editor | 🧪 Partial | ⭐⭐⭐⭐ | macOS only, uses a separate plugin instance, and polls parameters. |
| Plugin parameter UI | 🧪 Partial | ⭐⭐⭐⭐ | Generic parameter list. UUID to `u32` conversion truncates plugin parameter IDs. |
| Plugin state save and restore | ✅ Implemented | ⭐⭐⭐⭐⭐ | Native editor state is saved on close/exit, stored under `state/` in the bundle, and restored when plugin instances are rebuilt. |
| Plugin MIDI output | 📋 Planned | ⭐⭐⭐ | Output events are collected and discarded. |
| Sidechain and multi-out buses | 📋 Planned | ⭐⭐⭐⭐ |  |
| Plugin latency compensation | ✅ Implemented | ⭐⭐⭐⭐⭐ | The plan and engine compensate non-zero device latency, including latency queried from vendored VST3/VST2/AU wrappers. |
| Out-of-process hosting | 🧪 Partial | ⭐⭐⭐⭐ | Native audio, plugin DSP, and plugin editor windows run in one supervised engine child process. Per-plugin/per-vendor sandbox settings remain planned. |
| Plugin crash recovery | ✅ Implemented | ⭐⭐⭐⭐ | A child crash leaves the UI running, shows the restart overlay with a Restart button, and rebuilds from the canonical project. Per-plugin restart remains planned. |
| Plugin preset management | 📋 Planned | ⭐⭐⭐ |  |

## Extensions

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Extension host | ✅ Implemented | ⭐⭐⭐⭐ | Native shared-library host plus `wasmtime`/browser WebAssembly host, ordered target selection, user-directory discovery. |
| Extension device DSP | 🧪 Partial | ⭐⭐⭐⭐ | Block `f32` processing, parameters, dynamic latency, and note events are forwarded. Persistent device state blobs are not. |
| Extension device panels | ✅ Implemented | ⭐⭐⭐⭐ | Guest panel frames use the real egui API with shared `notwig-ui` components and validated actions. |
| Extension sandboxing | ✅ Implemented | ⭐⭐⭐⭐ | WASM always runs with memory and fuel limits. Native shared libraries run trusted with no sandbox, as documented. |
| Extension registry and browser | ✅ Implemented | ⭐⭐⭐⭐ | Settings explorer has All/Available/Installed filters, icons/descriptions, context actions, manage-index window, and configurable index sources. |
| Extension install and update | ✅ Implemented | ⭐⭐⭐⭐ | Install from index URLs/paths or local ZIPs, confirm reinstall, uninstall cleans files and settings. |
| Extension commands | 🧪 Partial | ⭐⭐⭐⭐ | Manifest v2 command contributions, bounded command context, host edit families, command palette (F1 / Cmd+Shift+P) with muted unavailable commands, string-keyed bindings, WASM/native/browser execution, and the `notwig-ext-example` humanize command. The audio editor has host-owned Audio Events/Gain sub-editors; extension-contributed sub-editors and panels remain follow-ups. |
| General drawing hooks | 📋 Planned | ⭐⭐⭐ | Arranger clips, track headers, lanes, and editors on the shared draw IR. |
| Official extension index | 🧪 Partial | ⭐⭐⭐ | `docs/extensions.csv` builds `docs/extensions.json` with per-target URLs and SHA-256 hashes; CI packages macOS Apple Silicon and WASM builds and publishes via GitHub Pages. |

The broader Eclipse-style extension-point model is not scheduled. The proposed
first migration (extension commands and a palette, audio-editor sub-editors, and
extension panels), determinism rules, clipboard service design, and migration
risks are analyzed in
[`Extension Platform Architecture`](23_extension_platform_architecture.md).

## UI and workflow

| Feature | Status | Importance | Notes |
| --- | --- | --- | --- |
| Transport bar | ✅ Implemented | ⭐⭐⭐⭐⭐ | Play, stop, loop, tempo, time signature, position, master meter, CPU load, settings. |
| Arranger | ✅ Implemented | ⭐⭐⭐⭐⭐ | Tracks, clips, time selection, loop, grid, playhead, drop targets. |
| Contextual inspector | 🧪 Partial | ⭐⭐⭐⭐ | Some audio fade and gain fields are read-only. |
| Device panel | ✅ Implemented | ⭐⭐⭐⭐⭐ | Chain cards, bypass, nesting, EQ editor, generic parameter knobs. |
| Audio editor | ✅ Implemented | ⭐⭐⭐⭐ | Audio Events and Gain sub-editors. Also see the Audio table. |
| MIDI editor | ✅ Implemented | ⭐⭐⭐⭐⭐ | Opens for MIDI clips from the arranger and the clip editor toggle. |
| Mixer view | 🚫 Not Planned | ⭐⭐ | Not required in core. Mixing works through track headers and the inspector. |
| Automation editor | 📋 Planned | ⭐⭐⭐⭐⭐ |  |
| Project and asset browser | 📋 Planned | ⭐⭐⭐ | Only the device browser exists. |
| Settings | ✅ Implemented | ⭐⭐⭐⭐ | Audio, Shortcuts, and Extensions pages. |
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

When a feature changes status, edit the row and its notes. Move an item to ✅ Implemented only after the test exists. When scope is dropped, move the item to 🚫 Not Planned or to the extension points list and record the reason in the relevant design document. Test coverage is tracked separately in [`Testing`](05_testing.md).
