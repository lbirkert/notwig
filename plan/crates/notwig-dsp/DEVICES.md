# DSP Device Contracts (`notwig-dsp`)

## 1. Definitions
- `DeviceKind ::= Instrument | AudioFx | NoteFx | Container`
- `Id<T>`: stable opaque identifier (from core).
- `SchemaVersion ::= { major: u16, minor: u16 }`
- `Latency ::= { samples: u32 }` (measured at current `sr`/config; runtime-only)
- `StateBlobRef`, `ParamValue`, `OrderedMap`, and `Id<Device>`/`Id<DeviceChain>` come from `notwig-core`. `Id<Device>` identifies a persisted `DeviceInstance`.

## 2. Device Spec vs Instance
- `DeviceSpec` (derived, not persisted): static description resolved from `(typeId, version)` by the device registry: kind, ports, ordered `ParamSpec`s, latency behavior, UI metadata, and state compatibility.
- `DeviceInstance` (persisted in `S_doc`):
```
DeviceInstance = {
  id: Id<Device>,
  typeId: TypeId,                 // stable, namespaced e.g. "notwig.eq", "vst3.<uid>", "au.<uid>"
  version: SchemaVersion,
  name: Option<String>,
  paramValues: OrderedMap<Id<Param>, ParamValue>,
  stateRef: Option<StateBlobRef>, // opaque plugin/device state in bundle state/
  bypass: bool,
  childChains: Vec<Id<DeviceChain>> // for Container devices
}
```
- Schema ownership: the persisted `DeviceInstance`/`DeviceChain` structs are defined by `notwig-core`; this document specifies their DSP semantics.
- `typeId` + `version` define parameter/state compatibility domain. State MUST be sufficient for deterministic restoration given a compatible implementation.
- Instances are validated against their resolved `DeviceSpec`; unknown `(typeId, version)` instances load as unresolved with an explicit issue and cannot be inserted/activated until resolved.
- `RemoveDevice` removes the instance and recursively deletes its owned child chains/devices; state blobs become unreferenced. Removing the sole instrument of an Instrument track leaves it silent until an instrument is inserted.
- `Latency` is not document state: the engine measures/reports it in `S_rt` (`RtEvent::LatencyChanged`) and recomputes PDC on plan changes.

## 3. Device Chains
`DeviceChain = { id: Id<DeviceChain>, devices: Vec<Id<Device>> }`
- v0: ordered serial list `d1 -> ... -> dn`; each track owns exactly one main chain.
- `Instrument` tracks: zero or one `Instrument` device in the main chain; when present, NoteFx before it and AudioFx after it. Zero instruments is valid and silent. No stock instrument exists in v0; instrument devices are hosted plugins.
- Container devices own child `DeviceChain`s; nesting forms a tree (no cycles).
- Evaluation order is total serial traversal order.

## 4. Ports & Signal Types
`SignalType ::= Audio{ch:u16} | Midi | NoteEvent | SidechainAudio{ch:u16}`
`Port = { id: PortId, dir: In|Out, signal: SignalType, name: String, optional: bool }`

Compatibility: `(p_out,p_in)` valid iff `dir` matches and signal types/channels compatible per device contract.
- v0 performance uses `NoteEvent` for clip/playback data; `Midi`/CC/sysex paths are reserved.
- `Instrument`: consumes `NoteEvent` (v0; `Midi` reserved), produces `Audio`.
- `AudioFx`: consumes/produces `Audio` (+ optional sidechain in).
- `NoteFx`: consumes/produces `Midi|NoteEvent`.
- `Container`: exposes external ports mapped to internal endpoints.

## 5. Processing Interface (abstract, RT-safe `process`)
For instance at config `C=(sr, maxBlock, channelLayout)`:
- `prepare(C) -> Result<Prepared, E>` (non-RT)
- `reset()` (RT-safe)
- `apply_param(id, value, ctx)` (RT-safe per `PARAMETERS.md`)
- `process(ctx, io_audio, events) -> ProcessStatus` (RT-safe)
- `latency() -> Latency`; state capture/restore non-RT unless explicitly RT-safe
- Bypass is click-free and latency-compensated: a bypassed device passes dry signal while retaining the reported latency so PDC stays stable; aggregate chain latency includes contained devices.

`ctx` provides transport, tempo (constant v0), sample position, block size; no UI/document pointers.
- Instrument devices receive notes only in v0 (pitch/timing/velocity; no channel/CC/PB/PC/sysex).

## 6. Extensibility & Compatibility
- New device types added via new `typeId` without breaking existing contracts.
- Migration `M_{(typeId,v)->(typeId,v')}` MAY transform state/params; incompatible if absent.
