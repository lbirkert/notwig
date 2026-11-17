# Parameter Contracts (`notwig-dsp`)

## 1. Parameter Model
`ParamSpec = {
  id: ParamId,                    // stable within typeId
  name: String,
  kind: Continuous | Discrete | Bool | Enum,
  unit: Unit,
  range: RangeSpec,               // phys min/max, steps, or enum values
  default: Value,
  normMap: NormMap,               // phys <-> norm [0,1], total and deterministic
  smoothing: Option<SmoothingSpec>,
  flags: ParamFlags
}`

`ParamFlags = { automatable: bool, visible: bool, modulatableReserved: bool, macroAssignableReserved: bool }`
- `ParamSpec` is derived from `DeviceSpec` for a `(typeId, version)` and is not persisted; only physical values (`DeviceInstance.paramValues`) are stored. Normalized values are a UI/host projection.
- `normMap` invertible within representable precision and consistent.
- `ParamId` stability required for persistence, host mappings, and future automation/modulation.
- Unknown persisted parameter IDs are retained where possible and reported; they MUST NOT be applied to processing until the spec resolves them.

## 2. Value Domains & Updates
- `Value = Phys(...) | Norm(f32 in [0,1])` (kind-consistent).
- Update: `ParamSet { deviceId, paramId, value, sampleOffset?: u32, ramp?: RampSpec }`.
- v0 semantics: applied at block boundary or supported sample offset; last-write-wins per target; optional smoothing `S(prev,target,dt)` for continuity/click avoidance.
- Out-of-range/unknown => clamp or reject per spec and report diagnostic (non-RT); RT path MUST remain stable.

## 3. Automation (reserved runtime v0)
- Document model: automation source `A(paramId): t_m -> Value` (lane/curve) - structure reserved.
- Future: sample-accurate evaluation when supported; else control/block rate. Automation and modulation remain distinct sources.

## 4. Modulation & Macros (reserved runtime v0)
- `ModEdge = (src, dstParamId, depth, shape/mode)` sources may include LFO/envelope/note expr/macro etc. in future.
- Effective value concept: `v_eff = clamp(combine(base, mods))` per device rules.
- v0: reserved fields/IDs MUST NOT affect processing unless explicitly version-enabled.

## 5. RT & Safety
- RT parameter application uses preallocated storage/queues and fixed lookup; no allocation/locks in callback.
- Parameter identity and mapping shared with UI/host via descriptors, not ad-hoc logic.
