# Protocol Contracts (`notwig-core`, v0)

Canonical cross-boundary message contracts. All messages are data-only, serializable/loggable in principle, and carry no UI/RT pointers.

## 1. `Command` (UI/App/Import -> Core)
`Command = { id: CmdId, payload: CommandPayload }` (id optional but recommended for correlation)

Payload families (v0, non-exhaustive but canonical categories):
- **Project/Transport:** `SetTransport{playing?, playheadMus?, loopEnabled?, loopRange?}`, `Pause`, `Stop` (Stop -> loop start or 0). Tempo/time-signature are fixed per project and have no edit command in v0.
- **Tracks:** `CreateTrack{id,chainId,kind,name,parent?,index?}`, `DeleteTrack{id}`, `RenameTrack{id,name}`, `MoveTrack{id,parent?,index}`, `SetTrackMixer{id, volumeDb?, pan?, mute?, solo?}`, `SetTrackRoute/Send{...}`
- **Sequencer:** `CreateClip{id,trackId,kind,range,payload}` (payload notes carry IDs), `EditClip{clipId, range?, name?, muted?, contentStart?, contentLength?, audioParams?}`, `MoveClip{clipId, trackId?, range}`, `SplitClip{clipId, at, newClipId, newNoteIds}`, `DuplicateClip{clipId, newClipId, noteIdMap, trackId?, range?}`, `DeleteClip{clipId}`, `SetNote{clipId,note}` (note carries `id`), `DeleteNote{clipId,noteId}`
- **Devices/Params:** `InsertDevice{id,typeId,version?,targetChain,index?,childChainIds?}`, `RemoveDevice{deviceId}`, `MoveDevice{deviceId,to,index}`, `SetDeviceBypass{deviceId,bypass}`, `SetParam{deviceId,paramId,value:Phys|Norm,ramp?}`
- **Assets:** `ImportAsset{assetId, path, options?}` (project service performs ingest), `RegisterAsset{assetId, assetMeta}`, `RelinkAsset{assetId, uri}`

Rules:
- Core validates and applies atomically; unknown/invalid => `CoreError`, no mutation.
- All create/duplicate/split payloads MUST carry caller-generated IDs; Core validates uniqueness and never allocates IDs. `newNoteIds`/`noteIdMap` enumerate every synthesized note ID, and `childChainIds` must be new unique chain IDs (created empty by the command).
- v0 has no recording/live-input/arm/monitor commands; such fields are reserved and inert.
- MIDI clip edits carry notes only (no channel/CC/pitch-bend/program-change events).
- Device insertion validation: at most one `Instrument` per Instrument track; `Instrument`/`NoteFx` rejected on non-Instrument tracks; AudioFx allowed on any track chain.

## 2. `StateDelta` and Queries (Core -> UI/App)
- `StateDelta = { rev: u64, changes: Vec<DocChange>, affected: AffectedSet }`
  - `DocChange` is a typed patch/event (insert/update/remove of entities or fields) sufficient for projections to update; full `Snapshot{rev, doc}` allowed on load/sync.
- `Query -> QueryResult` (read-only, from `S_doc` and optionally last-known `S_rt` snapshots):
  - examples: `GetProject`, `GetTrack{id}`, `GetArrangerRange{range}`, `GetDevice{id}`, `ListAssets`
  - subscription model: observers receive `Snapshot|StateDelta` stream ordered by `rev`.

## 3. `EngineMsg` (Core -> Engine, prepared, bounded)
Applied at safe boundaries (block boundary/swap point), never in-place mutation of `S_doc`:

- `ApplyPlan{plan: Arc<CompiledPlan>}` (immutable/prepared graph/route plan; includes computed PDC delays)
- `SetTransport{playing, playheadSamples, stop?}` (Stop -> loop start/0; runtime playhead authority transfers to the engine)
- `SetParam{deviceId, paramId, valueNormOrPhys, ramp?}`
- `UpdateSchedule{arrangerWindowOrEvents}` (derived playback schedule/events/assets refs; notes-only MIDI in v0)
- `SetTrackState{trackId, mute, soloEffective, gain, pan, ...}` (engine-facing resolved controls)
- `PrepareAssetReaders{clipId, assetId, region}` (assets are already at project sample rate)

Delivery: bounded, ordered FIFO control queue; non-blocking try-send from non-RT producers where required, with explicit overflow handling/reporting.
Deterministic in-block ordering: events apply at their sample offset; at equal offsets, note-offs precede note-ons, then notes by `Id`, then other control events by queue arrival order.

## 4. `RtEvent` (Engine -> Core/UI, bounded, RT-safe emission)
- `TransportTick{playheadSamples, playheadMusApprox}`
- `Meters{trackId|busId, peaks, rms}`
- `Status{xruns, cpuLoad?, config}`
- `LatencyChanged{deviceId|graph, samples}` (latency is runtime/config-dependent and never persisted in `S_doc`)
- `PluginEvent{deviceId, kind, payloadRef?}` (non-RT detail fetched out-of-band)
- `Error{scope, code, messageId}` (diagnostic; no allocations in RT emission path)

Delivery: bounded lock-free queue/ring; lossy policy allowed only for explicitly droppable telemetry (e.g. meters) but not for critical state-change acknowledgments (delivered via control path). `TransportTick` is the authoritative runtime playhead observation.

## 5. Correlation & Consistency
- `rev`/message ordering establishes document projection order; `S_rt` streams are time-based and may interleave visually with `S_doc` updates.
- UI MUST treat `S_doc` deltas as semantic truth and `S_rt` as runtime observation (per high-level invariants).
