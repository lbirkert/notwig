# High-Level Architecture & Contracts (v0)

Merged overview of system architecture and cross-cutting contracts. Precise subsystem details live under `plan/crates/notwig-*`; workspace-level planning lives under `plan/code/`.

## 0. Scope
- Clean-room DAW inspired by Bitwig Studio interaction model and device concepts. No code/assets copied from Bitwig or Hikaru OpenLive.
- v0 priority: small set of high-quality, extensible fundamentals over feature breadth.
- **Excluded in v0:** Clip Launcher/scenes, comping/takes, elastic audio warping, tempo/time-signature automation, modulation/macros execution, undo/redo, collaboration, out-of-process plugin hosting.
- **Playback/edit only:** no audio/MIDI recording and no live input capture or monitoring. `transport.recording`, track `arm`/`monitor`, and input endpoints are reserved and inert.
- **No stock instrument:** Instrument tracks require a hosted VST3/AU (VST2 fallback) instrument device; no built-in synth/sampler in v0.
- **MIDI is notes-only:** no MIDI channel, CC, pitch bend, program change, or sysex in clip data; per-note expression remains reserved.
- **No metronome/count-in** in v0.
- **Target platform:** macOS first (VST3 + AU; VST2 fallback where available), with OS-specific code behind adapters for later Windows/Linux.

## 1. Resolved v0 Decisions
| Topic | Decision |
|---|---|
| Clip Launcher / scenes | Excluded from v0 prototype |
| Clip identity (Arranger/Launcher) | N/A in v0; all sequencer clips are Arranger instances |
| Track kinds | `Audio, Instrument, Group, BusFx, Master` |
| Comping/takes | Deferred entirely (no take-aware schema in v0) |
| Note expression / MPE | Extensible per-note expression represented in core contracts; no capture/UI in v0 |
| Audio warping | Basic trim/fades/gain only, non-destructive; no warp markers/stretch in v0 |
| Tempo/time-signature | Fixed for the project lifetime in v0; `TempoMap`/schema reserved for future changes/automation/ramps |
| Modulation/macros | Deferred in runtime; parameter/device contracts reserve modulatable/macro targets |
| Plugin hosting | In-process only in v0; hosted instruments only (no stock instrument) |
| Project persistence | Canonical directory bundle/package |
| Recording | Deferred in v0; playback/edit only (recording/arm/monitor fields reserved, inert) |
| Live input | Deferred in v0; no MIDI/audio input capture or monitoring |
| MIDI clip events | Notes only; no channel/CC/pitch-bend/program-change/sysex; per-note expression reserved |
| Metronome / count-in | Excluded in v0 |
| Target platforms | macOS first; portable architecture for later Windows/Linux |
| User configuration | User-level settings in platform config/cache dirs; project bundle stores project state only |
| Audio assets | Import WAV/AIFF, resampled offline to project sample rate; managed copies in bundle |
| Device persistence | Persisted `DeviceInstance` separated from derived `DeviceSpec`; latency is runtime (`S_rt`) |
| ID allocation | Caller-supplied stable IDs in create/duplicate/split commands; Core never allocates IDs |
| Transport playhead | Engine owns runtime playhead; `S_doc` stores locate/last-saved position; Stop returns to loop start (or 0) |
| Undo/redo | Deferred in v0 |
| Collaboration readiness | No collaboration-specific design in v0 (stable IDs retained as general invariant) |

## 2. Global Invariants
- `I1 Single Source of Truth`: each domain object has one canonical identity and state `S_doc(id)`; all views (Arranger, Mixer, Device Panel, Editors) are projections `π(S_doc, S_rt)`.
- `I2 Layering`: `UI  ⇄(Commands, Queries/Subs)⇄  Domain Core  ⇄(RT Messages)⇄  Audio Engine  ⇄  OS Audio I/O` (MIDI I/O deferred in v0).
- `I3 Real-time Safety`: audio callback path performs no heap allocation, blocking locks, filesystem/IPC, compilation, plugin scan/load/GUI, or unbounded work.
- `I4 Determinism`: offline render and realtime playback of identical document + input/event schedule produce equivalent audio within defined numeric tolerance `ε(cfg)`.
- `I5 Stable Identity`: all persisted domain entities addressed by stable opaque IDs; references by ID, never by positional index.

## 3. System Topology
```text
+-------------------+      Cmd/Query       +----------------------+
| egui + wgpu UI    | <------------------> | Domain Core (Doc)    |
| views/editors     |      StateDelta      | validation, tx apply |
+-------------------+                      +----------+-----------+
                                                     | EngineMsg (prepared, bounded)
                                                     v
                                           +----------------------+
                                           | Engine Control (non-RT)
                                           | graph build/swap, PDC |
                                           +----------+-----------+
                                                     | block-boundary swap
                                                     v
+-------------------+      RT events       +----------------------+
| OS Audio I/O      | <------------------> | Audio Engine (RT)    |
+-------------------+                      | DSP graph, scheduler |
                                           +----------+-----------+
                                                     | in-process calls
                                                     v
                                           +----------------------+
                                           | Device/Plugin Instances
                                           +----------------------+
```
- **v0 I/O:** audio output only; MIDI/audio input, monitoring, and external MIDI output are deferred.
- **Command flow:** UI emits intent `Cmd`; Core validates against `S_doc`, applies atomic mutation to `S_doc`, emits `StateDelta` and derived `EngineMsg`.
- **State split:** `S = S_doc ⊕ S_rt ⊕ S_ui`. `S_doc` authoritative project state; `S_rt` engine-owned timing/meter/playback state; `S_ui` ephemeral view state.

## 4. Time & Scheduling
- **Domains:** `t_a ∈ ℕ` samples at sample rate `sr`; `t_m ∈ ℚ_{>=0}` beats.
- **v0 mapping:** fixed tempo/time-signature `Meta.tempoBpm`, `Meta.timeSigNum/Den` define affine map `f(t_m)=t_a = round(t_m * sr * 60 / tempoBpm)` (and inverse). Contract reserves extensible `TempoMap` interface for future automation/ramps/signature changes without schema breakage.
- **Transport:** `Tr = (playing, playhead_m, loopEnabled, loopRange_m?)`; `recording` is reserved and inert in v0.
- **Playhead authority:** during playback the engine advances the authoritative runtime playhead in `S_rt`; `S_doc.transport.playheadMus` records locate/last-saved position only. Pause stops in place; Stop returns to `loopRange.start` when looping, else `0`.
- **Sample rate:** `Meta.sampleRate` is the project rate for the document lifetime in v0. Imported audio is converted offline before use, so no implicit sample-rate conversion runs in the engine.
- **Tempo/time-signature:** fixed for the project lifetime in v0 (set at creation/load, immutable thereafter); no edit commands are exposed. This avoids audio-duration remapping without warping.

## 5. Core Subsystems
| Subsystem | Responsibility | v0 contract |
|---|---|---|
| Project | lifecycle, persistence, assets, templates | Directory bundle, autosave/recovery, versioned manifest; WAV/AIFF import resampled offline to project rate |
| Tracks | track types, ordering, routing, mixer projection | Audio/Instrument/Group/Bus-FX/Master, static routing graph; lanes only on Audio/Instrument |
| Sequencer | linear arrangement and playback regions | Arranger lanes + clips only; no recording or content looping |
| MIDI | note/event model and piano-roll editing data | Notes only (pitch/velocity/timing); per-note expression reserved; no channel/CC/PB/PC |
| Audio | audio assets and clip edits | Non-destructive trim/fades/gain at project rate; no warp/stretch |
| Devices | processing abstraction and chains | `DeviceSpec`/`DeviceInstance` split; EQ, Tool, Chain stock FX; hosted instruments |
| DSP/Hosting | RT processing, graph, plugin/device execution | In-process VST3/AU (VST2 fallback); Faust-preferred stock DSP path (phased) |

## 6. Interface Contracts (summary)
- `UI -> Core`: `Command` (pure intent, serializable/loggable in principle; create/duplicate/split commands carry caller-generated IDs)
- `Core -> UI`: `QueryResult | StateDelta | Event`
- `Core -> Engine`: `EngineMsg` (immutable/prepared payloads, bounded transport)
- `Engine -> Core/UI`: `RtEvent` (bounded, RT-safe emission; engine-owned runtime playhead/latency)
- `Core <-> Storage`: `Manifest/ProjectDoc/AssetEntry` load-save with migration and validation
- `App <-> Settings`: user-level config in platform config/cache dirs (audio device, buffer/rate, plugin paths, caches, UI prefs, recent projects)
- `App/Project <-> Import`: WAV/AIFF decode + offline resample to project rate before asset registration

## 7. Validation & Error Model
- All external inputs (UI commands, project files, plugin descriptors/states) validated before mutation/application.
- Failure classes: `InvalidCommand`, `SchemaViolation`, `MissingAsset`, `UnsupportedVersion`, `PluginError`, `EngineConfigError`.
- RT path reports errors asynchronously via `RtEvent`; must not throw across audio callback boundary.

## 8. Extensibility Hooks (deferred execution)
- `TempoMap` replaceable from constant to piecewise/automated.
- `Param` model reserves modulation/macro sources/targets and controller mappings.
- Clip/payload schemas versioned for future warping, takes, launcher references, content loop/repeat, and MIDI channel/CC/pitch-bend/program-change events.
- Track/device graph abstractions permit future hybrid behaviors and additional containers.
- Recording/takes, live input, stock instrument, metronome, and Windows/Linux backends are reserved scope for later versions.
- Bundle manifest reserves sections for undo journal, collaboration metadata, plugin cache indices (unused in v0).

## 9. Out of Scope / Non-goals (v0)
- Full Bitwig feature parity, launcher performance workflows, advanced modulation grid, hybrid tracks, sandboxed plugin processes, collaborative editing, built-in marketplace/browser ecosystem, GPU DSP execution.
- v0 additionally excludes audio/MIDI recording, live input monitoring, stock instruments, metronome/count-in, Windows/Linux shipping builds, and sample-rate conversion during playback.
