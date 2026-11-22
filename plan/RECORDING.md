# Recording plan

Status: proposed. This is a planning note, not stable user documentation.

Related:

- [`docs/03_development/06_roadmap.md`](../docs/03_development/06_roadmap.md)
- [`docs/03_development/00_architecture.md`](../docs/03_development/00_architecture.md)
- [`docs/03_development/02_audio_engine.md`](../docs/03_development/02_audio_engine.md)
- [`docs/01_users/08_transport_and_render.md`](../docs/01_users/08_transport_and_render.md)
- [`docs/01_users/05_audio_clips.md`](../docs/01_users/05_audio_clips.md)

## 1. Clarified decisions

| Topic | Decision |
| --- | --- |
| First milestone content | Audio recording only. MIDI input and MIDI recording follow later. |
| Result of one pass | Replace the destination range on the armed track, then insert one audio clip for the pass. |
| Transport features | Linear recording only. Loop recording, punch in/out, and count-in are deferred. |
| Inputs | Per-track input selection, limited to one input channel per recording track. |
| Monitoring | Armed tracks monitor their input whenever they are armed. Monitoring passes through the armed track's device chain, mixer, and routing. |
| Stored channel layout | Store the selected channel as a mono capture, then duplicate it to stereo when finalizing the asset, matching the import pipeline. |
| Platforms | Native and browser recording. Browser support is part of this milestone, not a later milestone. |

## 2. Scope

### In scope

- Record arm per audio track.
- Per-track input device and single input channel selection.
- Input monitoring through the track chain while armed.
- Linear recording from the play start to Stop.
- Replace existing clips in the recorded range on the armed track.
- Persist the take as a normal project audio asset with waveform data.
- Native and browser implementations.
- One undoable edit for the completed take after finalization.
- Clear errors for missing input devices, missing permissions, unsupported sample
  rates, dropped input buffers, and records that produce no samples.

### Out of scope

- MIDI recording.
- Loop recording, punch in/out, count-in, pre-roll, and retrospective capture.
- Take lanes, comping, and multiple takes per destination range.
- Monitoring modes (off/auto/on). Monitoring is tied to record arm for now.
- Stereo-pair recording, multi-channel recording, and per-track gain staging before
  the input tap.
- Sample-rate conversion during live monitoring.
- Cross-device sample-clock drift correction.
- Out-of-process engine capture. The engine-process path must keep its API shape,
  but the first implementation targets the current in-process audio path.

## 3. Target workflow

1. User selects an audio track.
2. The track inspector shows an input picker: driver, device, and one channel.
3. User arms the track from the track header or inspector.
4. Arming opens the input stream and starts monitoring through the track chain.
5. User presses Record.
6. The app starts playback and recording at the play start (or current
   playhead if playback is already running).
7. The recorder captures the selected channel into a temporary take.
8. User presses Stop.
9. The app finalizes the capture into a stereo float WAV asset, registers it,
   clears the destination range on the armed track, and inserts the recorded
   clip.
10. The clip is selected and the whole take is one undo step.

If the project has no bundle path yet, finalization must first require Save As
and keep the temporary capture until the save succeeds.

## 4. Architecture

```text
CPAL input callback
  -> InputHub per-input ring
       -> monitoring ring  ---> Engine.process
       |                           -> armed track input buffer
       |                           -> track devices -> mixer -> routing -> master
       -> recording ring   ---> RecordingWriter thread
                                   -> temporary mono take
                                   -> finalize: resample if needed
                                   -> duplicate mono to stereo
                                   -> hash + write assets/<hash>.wav
                                   -> register asset + waveform
                                   -> clear destination range + create clip
```

The input callback only copies samples into preallocated bounded queues. The
writer thread owns allocation, file I/O, encoding, and finalization. The engine
callback owns monitoring injection and transport synchronization.

### Why monitoring goes through the engine

Direct app-side monitoring would bypass track devices, gain, pan, mute/solo,
routing, and master processing. The selected design matches normal DAW software
monitoring. It also keeps a single place for input-to-playhead alignment.

### Current audio path

The current native app owns an in-process `AudioService` in
`crates/notwig-app/src/audio.rs`; the child engine process exists but the GUI
still uses the in-process service. The recording API should live on
`AudioService`/`InputHub` so the child path can later reuse it. If engine-process
audio is wired before recording lands, add an IPC request for input
configuration and return capture chunks/status through the existing reply
channel.

## 5. Domain and protocol changes

### Track state

Add to `TrackIO` in `crates/notwig-core/src/tracks.rs`:

```rust
pub struct AudioInputRef {
    pub driver: String,
    pub device: String,
    pub channel: u16,
}
```

Replace or supersede the currently unused `audio_in: Option<String>` with
`audio_input: Option<AudioInputRef>` and `#[serde(default)]`. Validate that:

- the channel is within the device's enumerated input count at use time;
- only audio tracks can arm or select audio input;
- input selection does not require the device to be present on load, but arming
  fails with a clear unavailable-input error.

Add to `Track`:

```rust
#[serde(default)]
pub record_armed: bool,
```

Record arm is a document edit and is undoable. It is normalized to `false` when
loading or saving, alongside the existing volatile transport fields, so a
reopened project does not start with live inputs open.

### Transport

`TransportDoc.recording` already exists in `crates/notwig-core/src/domain.rs`.
Use it as the canonical transport recording state.

Extend `CommandPayload::SetTransport` with `recording: Option<bool>` and apply it
in `crates/notwig-core/src/apply.rs`. Recording changes must not enter the undo
stack, matching `playing`, `playhead`, and loop changes.

### Commands

Add:

- `SetTrackRecordArm { id, armed }`
- `SetTrackInput { id, input: Option<AudioInputRef> }`

`SetTrackInput` should be a normal undoable document command. `SetTrackRecordArm`
is undoable as a document command but does not trigger an engine plan rebuild.

### Engine messages and control data

Extend `EngineMsg::SetTransport` and `TransportUpdate` with `recording: bool`.

Add engine input configuration to the control path:

```rust
pub struct TrackInputRoute {
    pub track_id: TrackId,
    pub armed: bool,
    pub input: Option<AudioInputRef>,
}
```

The app publishes the armed routes to the engine when arm or input changes. The
engine uses the routes to consume monitoring frames from the input hub and to
apply input gain/pan/mute state through the existing track runtime.

### UI intents

Add `UiIntent` values for:

- `ToggleRecord`
- `SetTrackRecordArm`
- `SetTrackInput`

Track input changes become commands in `Workspace`; arm changes become commands
too, with side effects for opening/closing input streams.

## 6. Input capture and monitoring

### Input enumeration

Extend `AudioService::available_options()` to enumerate input devices and
channels separately from output devices. Add runtime views such as:

```rust
pub struct AudioInputDeviceView {
    pub id: String,
    pub label: String,
    pub channels: u16,
}
```

Input options must be available to the track inspector without opening Settings.
Track input is stored by driver, device, and channel because the global audio
setting currently selects only the output device.

### InputHub

Add a focused module, for example `crates/notwig-app/src/audio_input.rs`:

```rust
pub struct InputHub {
    streams: BTreeMap<InputKey, InputStreamState>,
}

pub struct InputStreamState {
    monitor_queues: BTreeMap<TrackId, InputQueue>,
    recording_queues: BTreeMap<TrackId, InputQueue>,
}
```

- One CPAL input stream per unique driver/device pair.
- One preallocated queue per armed track for monitoring and recording.
- Queue capacity is a few seconds at the project sample rate; overflow is
  reported as an error and the take is marked invalid rather than silently
  truncated.
- The callback converts samples to `f32`, selects the configured channel, and
  pushes frames without allocating, locking, or doing file I/O.
- Input streams are opened on arm and closed on disarm or track deletion.

The first milestone requires the input stream sample rate to match the project
sample rate. If the input device cannot open at that rate, arming fails with an
actionable error. Real-time resampling is deferred.

Multiple devices are technically allowed, but only one input device should be
recommended for sample-accurate multi-track recording. Cross-device clocks can
drift; report that as a documented limitation and a later calibration feature.

### Monitoring injection

Add an input tap to `notwig-engine`:

- `Engine` owns the monitoring queues supplied by the app.
- During `process`, the engine drains one block per armed track and writes the
  samples into the track's input buffer before the track's device chain runs.
- The track still applies volume, pan, mute/solo, routing, and master processing.
- Monitoring is active whenever the track is armed, including while transport is
  stopped. The output device callback continues to run, so the engine continues
  to render monitor input.
- The monitoring path must never allocate. Queues and block buffers are prepared
  on the app/loader thread and swapped in through the existing control exchange.

### Recording tap

The recording tap captures the raw selected input channel before track devices.
The input hub writes to both the monitoring queue and the recording queue while
a take is active. The recording writer drains the recording queue on a normal
thread.

This avoids recording post-FX output and avoids doing file I/O in the audio
callback.

### Alignment and latency

- Attach a monotonic sequence number and frame count to each queued block.
- Start the take at the engine block boundary that first observes
  `recording = true` and `playing = true`.
- Drop queued monitor audio that predates the take start.
- Populate `AudioSnapshot.dsp.input_latency_samples` from CPAL input timestamps
  where the host exposes a usable clock. Apply compensation when writing the
  clip source window; otherwise record with zero compensation and report that
  accurate input-latency compensation is unavailable.
- Keep the first implementation at block-level alignment. Add a calibration
  offset or loopback measurement if hardware testing shows audible offsets.

## 7. Recording lifecycle

### Start

1. Validate at least one armed audio track with an available input.
2. If no bundle path exists, keep the capture in a temporary location and prompt
   for Save As during finalization.
3. Create a temporary take id and writer sink.
4. Publish `recording = true` and start playback if the transport is stopped.
5. Start draining the recording queues into the temporary mono take.

### During recording

- The transport and playhead follow the existing engine behavior.
- The armed track shows live input monitoring through its chain.
- No document edits happen until finalization, so undo history stays clean.
- Input overflow, device loss, or stream errors stop the take and surface a
  notification.

### Stop

1. Publish `recording = false` and stop playback using the existing Stop path.
2. Close the take writer and wait for queued frames to be consumed.
3. Validate that the capture contains frames.
4. Resample if the capture rate ever differs from the project rate. The first
   milestone should reject mismatched rates earlier, so this is a safety net.
5. Duplicate the mono capture to stereo.
6. Encode a float WAV, hash it, and write it to the bundle `assets/` directory
   using the same content-addressed conventions as
   `crates/notwig-project/src/assets.rs`.
7. Build the waveform pyramid.
8. Apply the take as one batch:
   - `RegisterAsset`
   - `ClearRange` on the armed track for the recorded range
   - `CreateClip` with the recorded audio payload
9. Select the new clip and publish the new schedule/assets to the engine.

### Batch apply

Add a `Workspace::apply_batch(Vec<CommandPayload>)`-style helper that:

- snapshots the document once for undo;
- applies commands in order without publishing between them;
- publishes one engine update at the end;
- records one history entry;
- rolls back cleanly if any command fails.

If a rollback is too invasive for the first pass, validate all inputs before
starting and treat apply failures as internal errors rather than partial edits.

### Destination replacement

`ClearRange` already trims and splits clips on a track for a range. Recording
reuses it rather than adding a second overlap algorithm. The recorded clip is
created after the clear, so the destination contains exactly the new take.

The clip payload is a normal `AudioClipData`:

- `source_start_sample = 0`
- `source_end_sample = recorded_frame_count`
- `gain_db = 0`
- no fades in the first milestone
- no slices

## 8. UI

### Transport bar

- Make the existing record button in `crates/notwig-ui/src/views/shell.rs`
  interactive.
- Add a Record shortcut, defaulting to `Cmd+R` on macOS and `Ctrl+R` elsewhere.
- Record target state: idle, armed, recording, finalizing, error.
- Stop finalizes an active take.

### Track header

- Make the painted record button in
  `crates/notwig-ui/src/views/arranger/track.rs` an arm toggle.
- Show armed state distinctly from mute/solo.
- Disable arming for non-audio and non-lane tracks.
- Keep the existing meter; add an input-level meter or reuse the track meter
  while monitoring if the engine can expose input peaks.

### Track inspector

- Add an Input section beside the existing record button in
  `crates/notwig-ui/src/views/info/track.rs`.
- Provide driver, device, and channel selectors.
- If no input is configured and the user arms the track, auto-select the first
  available input and show a status message.
- Show a clear unavailable state when the configured device or channel is
  missing.
- Replace the inert inspector record button with the same arm toggle as the
  header.

### Runtime projection

Extend `RuntimeView` with:

- recording state;
- finalizing/progress state;
- available input devices/channels;
- per-track input levels if available;
- recent recording errors.

All of this remains view data; canonical arm/input state stays in `ProjectDoc`.

## 9. Browser

- Enumerate input devices through CPAL's WebAudio host.
- Recording requires a user gesture and microphone permission. Surface a clear
  permission prompt and denied-permission state.
- Reuse the same intents, commands, and finalization logic.
- Store the finalized asset through the existing OPFS project storage path.
- Keep a bounded in-memory staging buffer on the UI/writer side; browser
  threading and SharedArrayBuffer support vary, so the first implementation may
  drain capture blocks on animation frames instead of using a native writer
  thread.
- Do not compile native input or asset paths into the wasm target.
- Verify with `cargo check -p notwig-web --target wasm32-unknown-unknown` and a
  manual browser test.

## 10. Milestones

### M0: Feasibility spike

- Open a CPAL input stream at the project rate on macOS and in the browser.
- Confirm channel selection, callback timing, and input timestamps.
- Confirm permission handling in the browser.
- Verify that input callbacks can push into a bounded queue without allocation.
- Result: a short throwaway harness or ignored integration test, plus documented
  timing findings.

### M1: Domain and transport

- Add `AudioInputRef`, track arm state, commands, validation, and serde defaults.
- Extend `SetTransport` with `recording`.
- Add UI intents and runtime recording state.
- Tests: command apply/validation, transport normalization, round-trip load.

### M2: Native capture and monitoring

- Implement `InputHub`, input enumeration, stream lifecycle, bounded queues, and
  the recording writer.
- Add engine input injection before the track device chain.
- Make record arm open and close input streams.
- Tests: queue overflow/error paths, engine input injection, monitoring respects
  mute/pan/volume, no-allocation checks where practical.

### M3: Finalization and replacement

- Implement WAV asset writing, hashing, waveform creation, and registration.
- Add batch apply and destination replacement.
- Tests: take finalization creates one asset and one clip, replaces overlapping
  clips, produces one undo entry, handles empty/overflowed takes, and requires
  Save As for unsaved projects.

### M4: UI and shortcuts

- Activate the top-bar record button and shortcut.
- Activate track header and inspector arm controls.
- Add input picker and recording state feedback.
- Add headless egui tests for arm toggling, input selection, recording visuals,
  and the finalizing state.

### M5: Browser recording

- Add WebAudio input enumeration and permission flow.
- Add browser capture staging and OPFS finalization.
- Verify wasm compilation and complete a manual record/finalize/playback pass.

### M6: Hardware validation and polish

- Record from the built-in microphone and an external interface.
- Verify monitoring through EQ/Tool/Chain, mute/solo, pan, and master processing.
- Measure record alignment with a loopback test and adjust latency compensation.
- Verify project reload, undo/redo, autosave, and missing-input recovery.
- Update user and development docs.

## 11. Test plan

| Layer | Tests |
| --- | --- |
| `notwig-core` | arm/input command validation, transport recording flag, serde defaults, invalid track kinds |
| `notwig-engine` | input injection ordering, monitor obeys mixer/routing, recording flag gating, queue underrun/overflow behavior |
| `notwig-app` | input enumeration mapping, stream lifecycle, writer drain, finalize asset, batch apply, one-step undo, unsaved-project handling |
| `notwig-ui` | arm button toggle, input picker, record button/shortcut, recording/finalizing visuals |
| Browser | wasm `cargo check`, permission-denied state, OPFS asset finalization |
| Manual hardware | mic and interface recording, monitoring chain, loopback alignment, device unplug/replug |

## 12. Risks and follow-ups

- **Clock drift across devices:** one input device is recommended until a clock
  calibration or resampling strategy exists.
- **Latency compensation:** CPAL timestamps do not share origins across streams
  on every host. Treat exact alignment as a calibration problem, not an
  assumption.
- **Engine process boundary:** the current GUI uses in-process audio. Keep the
  input API independent of that choice and define the IPC shape before wiring
  the child audio path.
- **RT safety:** every input callback path must remain allocation-free. The
  writer thread owns allocation and I/O.
- **Browser constraints:** microphone permission, sample-rate limitations,
  memory limits, and limited threading need explicit UI states.
- **Monitoring feedback:** warn when monitoring an input that is routed to the
  same physical output, and make it easy to disarm.
- **Future features:** takes/take lanes, comping, loop/punch recording,
  count-in, stereo-pair recording, MIDI recording, and per-track monitoring
  modes all build on this foundation but are intentionally deferred.

## 13. Expected file touch list

| Area | Files |
| --- | --- |
| Domain | `crates/notwig-core/src/tracks.rs`, `domain.rs`, `protocol.rs`, `apply.rs`, `lib.rs` |
| Engine | `crates/notwig-engine/src/engine.rs`, `control.rs`, `runtime.rs` |
| App audio | `crates/notwig-app/src/audio.rs`, new `audio_input.rs`, new `recording.rs` |
| App policy | `crates/notwig-app/src/workspace/mod.rs`, `workspace/assets.rs`, `gui.rs` |
| Project I/O | `crates/notwig-project/src/assets.rs` |
| UI | `crates/notwig-ui/src/intent.rs`, `runtime.rs`, `views/shell.rs`, `views/arranger/track.rs`, `views/info/track.rs`, `commands.rs` |
| Browser | `crates/notwig-web/src/lib.rs`, `crates/notwig-app/src/web/` as needed |
| Docs | user transport/audio-clip pages, audio engine, architecture, code structure, roadmap, crate READMEs |
