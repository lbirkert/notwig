# Engine Execution Contracts (`notwig-engine`)

## 1. Responsibilities
- Hold and execute active `CompiledPlan` and device/plugin instances.
- Drive transport-clocked scheduling for arranger playback (recording and live input are out of v0).
- Process audio/note events per block, route signals, and produce outputs (MIDI clips carry notes only).
- Measure and report device/plan latency for PDC; latency is runtime-owned, never document state.
- Emit `RtEvent` (transport ticks, meters, status/xruns, latency changes, diagnostics) to Core/UI.
- Apply `EngineMsg` from Core at safe boundaries (block boundary / atomic swap).

## 2. Boundaries & Ownership
- Consumes prepared/immutable data derived from `S_doc` and resolved `DeviceSpec`s; **never mutates `S_doc`**.
- Owns runtime state `S_rt`: authoritative playback playhead/transport, meters, config/status, latency, and instance runtime state.
- Pause stops in place; Stop returns to loop start (or 0). `S_doc.transport` holds locate/last-saved intent only.
- UI observes engine only via `RtEvent`/projections, never direct RT access.

## 3. Instance & Plan Lifecycle
- States (conceptual): `Unloaded -> Prepared -> Active -> Suspended/Bypassed -> Released`.
- Non-RT: prepare/instantiate/load/restore/compile/release, IO negotiation, heavy work.
- RT-safe as applicable: reset, param application, process, bypass transitions (click-minimized via smoothing/crossfade policy).
- Plan/config changes: build off-RT, `swap` atomically at block boundary with resync/PDC update.

## 4. Control Interface
- Input: bounded ordered queue of `EngineMsg` (see `crates/notwig-core/PROTOCOL.md`).
- Output: bounded queue/ring of `RtEvent`; droppable telemetry (meters) may be lossy under pressure, critical diagnostics/status handled per policy.
- Configuration: sample rate, block size, channel/device layout; changes trigger reprepare/rebuild outside RT and safe handover.
- The engine runs at `Meta.sampleRate`; v0 requires the audio device to open at that rate. If unsupported, playback is unavailable and an explicit error is surfaced (no hidden sample-rate conversion). Buffer size follows user settings.

## 5. v0 Scope
- In-process device/plugin execution (no sandbox IPC); hosted instruments only.
- CPU execution backend baseline; constant tempo/time-signature scheduling.
- Deterministic behavior given same plan, events, and config (within `ε`) for stock DSP and deterministic plugins.
- No recording, live input, metronome, or stock instrument in v0.
- Overload/error fallback: click-free fade to silence, continue processing, report status; never repeat stale buffers.
