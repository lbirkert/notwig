# Realtime & Offline Rendering (`notwig-engine`)

## 1. Block Processing (RT)
Per audio callback block `[t_a, t_a + N)`:
1. Drain/apply pending `EngineMsg` eligible for this boundary (transport/params/plan swap/schedule updates).
2. Resolve transport and playhead; compute active arranger regions/events via constant tempo map (v0).
3. Deliver timestamped events to instances.
4. Process nodes in `CompiledPlan.order` with preallocated buffers/routes and PDC delays.
5. Write outputs, produce RT-safe meters/taps, emit `RtEvent`s.

All steps on RT path must satisfy RT-safety invariants.

## 2. Transport & Playhead
- `S_doc.transport` expresses locate/intent; the engine maintains the authoritative runtime playhead in samples (and derived musical position) during playback (`S_rt`).
- Pause stops in place; Stop returns to `loopRange.start` when looping, else `0`. Locate while playing is sample-accurate and emits all-notes-off.
- Loop: wrap within `loopRange` deterministically at block/sample boundaries; wrap emits all-notes-off.
- Recording and live input capture are excluded from v0 (no capture/materialization path; `recording`/arm/monitor fields are inert).
- Notes only: no channel/CC/PB/PC/sysex; no note chase at start/locate; note-offs precede note-ons at equal offsets.

## 3. Offline Render
- Renders a specified `Range` from `S_doc` + assets to buffer/file using equivalent semantic plan/scheduling.
- v0 export is a Master mixdown to WAV at project rate, 32-bit float PCM (no stems/normalization/dither).
- Allowed: larger blocks, parallelism, non-RT allocations outside semantic equivalence boundary.
- Must align with RT within tolerance `ε(cfg, dtype, devices)` for identical inputs; output is block-size invariant for stock DSP.
- Hosted plugins may be non-deterministic and are excluded from bit-exact equivalence.

## 4. Configuration & Errors
- Block-size/channel-layout changes or device reopen at the project sample rate: stop or pause processing as needed, reprepare off-RT, swap/apply, resume.
- Audio-device hotplug/loss: pause processing, rebuild/reprepare on the new device, resume; report status to UI. If no device is available, remain silent.
- Xruns/overload/device errors reported as `RtEvent::Status/Error`; on overload the engine fades to silence, continues, and never outputs stale buffers.

## 5. Metering & Analysis
- Meter/analysis taps write into fixed preallocated buffers and emit via the bounded `RtEvent` path; meters may be lossy under pressure.
- Spectrum frames (provisional v0): 2048-sample FFT, 50% hop, stereo sum, decimated delivery at up to 60 Hz; analysis is not part of offline equivalence.
