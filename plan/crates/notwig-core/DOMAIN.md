# Domain Model (`notwig-core`)

## 1. Primitive Types
- `Id<T>`: opaque, globally unique within a project, stable for entity lifetime; never reused. Serialized as string UUIDv4 (canonical text form). References are by `Id<T>` only. Commands that create entities MUST carry caller-generated IDs; Core validates uniqueness and never allocates IDs.
- `SchemaVersion = { major: u16, minor: u16 }`.
- `Rational = { num: i64, den: u32>0 }` (normalized) for musical/time-signature exactness.
- `TimeMus = Rational` (beats, >= 0). `TimeAudio = u64` (samples).
- `RangeMus = { start: TimeMus, end: TimeMus }` with `end >= start`; intervals are half-open `[start,end)` for placement/playback unless stated.
- Units: `Db(f32)`, `GainLinear(f32)`, `Pan(f32 in [-1,1])`, `Norm01(f32 in [0,1])`, `Semitones(f32/i32 as specified)`, `Hz(f64/f32)`, `SampleRate(u32)`.
- `ParamValue = F64(f64) | I64(i64) | Bool(bool) | EnumIndex(u32)` (persisted physical parameter value).
- `OrderedMap<K,V>`: insertion-ordered map; iteration order is part of the persisted order.
- `StateBlobRef = { hash: String, encoding: Binary }` (content-addressed blob under bundle `state/`).
- `Color`: RGBA 8-bit or equivalent deterministic representation.
- `AssetEntry = { id: Id<Asset>, kind, uri, hash?, format, sampleRate?, channels?, durationSamples?, originalSampleRate?, originalChannels? }`; `AssetIndex = { assets: BTreeMap<Id<Asset>, AssetEntry> }`.

## 2. Document Root (`S_doc`)
`ProjectDoc = {
  schema: SchemaVersion,
  meta: Meta,
  transport: TransportDoc,
  tracks: BTreeMap<Id<Track>, Track>,      // stable iteration by Id for determinism where needed
  trackOrder: TrackForest,                 // ordered forest of Id<Track>
  arranger: ArrangerState,
  devices: BTreeMap<Id<Device>, DeviceInstance>,
  chains: BTreeMap<Id<DeviceChain>, DeviceChain>,
  assets: AssetIndex,
  routing: RoutingGraph
}`

`Meta = { name: String, createdUtc: i64, modifiedUtc: i64, sampleRate: SampleRate, tempoBpm: f64, timeSigNum: u8, timeSigDen: u8 }`
- v0: `tempoBpm` and `timeSig` are fixed for the project lifetime (no edit commands); schema reserves future `TempoMap` replacement.
- `sampleRate` is the project rate for the document lifetime. Imported audio is resampled to it before registration; changing it is not supported in v0. Runtime buffer size comes from user settings, not the document.

`TransportDoc = { playing: bool, recording: bool (reserved, inert), playheadMus: TimeMus, loopEnabled: bool, loopRange: Option<RangeMus> }`
- `playheadMus` is locate/last-saved position; the engine owns the authoritative runtime playhead during playback (`S_rt`).
- Stop semantics are protocol-level: return to `loopRange.start` when looping, else `0`; pause stops in place.
- When `loopEnabled`, `loopRange` MUST exist with `start < end`; invalid loop state is rejected.
- On load, `playing` is normalized to `false` (projects open stopped) and `recording` remains inert.

## 3. Mutation & Consistency
- `Command`: validated intent (see `PROTOCOL.md`).
- `apply(doc: &ProjectDoc, cmd: &Command) -> Result<ApplyResult, CoreError>`
  - `ApplyResult = { doc: ProjectDoc, delta: StateDelta, engine_msgs: Vec<EngineMsg> }`
- Atomicity: accepted command yields one coherent state; no partial application.
- Determinism: identical `(doc, cmd)` => identical result. This requires caller-supplied IDs in all entity-creating payloads; Core never allocates IDs or uses randomness.
- Validation (minimum): ID existence/uniqueness, duplicate-ID rejection, type compatibility, numeric/range bounds, structural constraints (exactly one `Master`, valid forest, no illegal cycles, lane eligibility), referential integrity (clips->tracks/assets, routing endpoints), note/content bounds, and v0 edit rules (e.g. arranger non-overlap resolution).
- Unknown `(typeId, version)` device instances are retained as unresolved on load, reported explicitly, and cannot be inserted/activated until resolved.

`CoreError = InvalidCommand{reason} | NotFound{id} | DuplicateId{id} | ValidationFailed{reason} | Conflict{reason} | Unsupported{reason}`

## 4. Projection Model
- Projection: pure `V = π_v(S_doc, S_rt, S_ui)`; views never own canonical edits.
- `S_doc`: authoritative, persisted semantics. Includes `TransportDoc` intent/locate position only.
- `S_rt`: engine-owned (authoritative playback playhead, actual transport timing, meters, xrun/status, latency reports); observed only.
- `S_ui`: ephemeral (selection, focus, viewport, drag/gesture, panel layout, transient notices); not semantic history in v0.

## 5. Identity & References
- All persisted domain entities addressed by stable `Id<T>`: tracks, clips, notes, devices, chains, assets, etc.
- Ordering is explicit (`trackOrder`, lane clip order); positional indices are not persisted identities.
- Deletion removes/updates references by validated command to preserve integrity.
- `AssetIndex`/`AssetEntry` schemas live here (because `S_doc` embeds them); `notwig-project` owns asset IO/ingest and blob storage.
- `DeviceInstance`/`DeviceChain` schemas live here:
  - `DeviceInstance = { id: Id<Device>, typeId, version, name?, paramValues: OrderedMap<Id<Param>, ParamValue>, stateRef?: StateBlobRef, bypass: bool, childChains: Vec<Id<DeviceChain>> }`
  - `DeviceChain = { id: Id<DeviceChain>, devices: Vec<Id<Device>> }`
  - DSP semantics are specified in `../notwig-dsp/DEVICES.md`; static `DeviceSpec`/`ParamSpec` descriptors are derived by `notwig-dsp` from `(typeId, version)` and are not persisted.
