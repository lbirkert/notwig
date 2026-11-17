# Processing Graph & Plan (`notwig-dsp`)

## 1. Graph Model
`ProcGraph G = (V, E)`:
- Nodes `V`: `{TrackInput, TrackOutput, DeviceNode, MixerSum, SendTap, MasterOut, Aux}`
- Edges `E`: directed typed connections `(src, port) -> (dst, port)` with `SignalType` compatibility.

Constraints:
- Audio edges type/channel compatible.
- Synchronous audio subgraph is a DAG in v0 (no feedback loops).
- Distinct domains: `Audio`, `Note/Midi/Event` (control/modulation reserved).

## 2. Compile to RT Plan (non-RT)
`compile(docView, engineCaps, config) -> Result<CompiledPlan, GraphError>`:
1. Expand tracks/chains/devices to nodes using resolved `DeviceSpec`s.
2. Validate structure/connectivity.
3. Compute node latencies `L(v)` from prepared instances/specs and compensation delays `Δ` for aligned mix points (PDC).
4. Topological order; assign buffers/voices from preallocated pools.
5. Emit immutable `CompiledPlan { order, buffers, delays, routes, paramBindings, scheduleMeta }`.

- `CompiledPlan` is derived only from `S_doc` + resolved specs + engine config. Latency is never read from `S_doc` (it is not stored there).
- Plans swapped atomically at block boundary. State transfer best-effort/defined per device where supported.

## 3. Buffers & Events
- Audio: preallocated buffers (f32 primary; f64 internal optional) valid within block unless specified.
- Events: bounded timestamped queues (sample offset in block) for note/MIDI/param/control.
- Avoid copies except for aliasing/write hazards; taps/meters emitted via RT-safe fixed-size outputs.

## 4. Latency & PDC
- Reported per device/chain: `L_total = Σ L_i` along path.
- Compensation inserted so synchronous arrivals align at mix/sum points within sample precision.
- Topology/latency changes trigger recompile and safe resync.

## 5. v0 Scheduling
- Constant tempo/time-signature map for musical->sample conversion.
- Per block `[t_a, t_a+N)`: resolve transport/schedule -> deliver events -> process `order` -> output/meters.
- Active arranger clips/notes derived from playhead interval and track routing; note events only (no channel/CC/PB/PC/sysex).
- Semantic output MUST be block-size invariant: for identical document/inputs, results within `ε` are independent of block size `N` except for explicitly documented stateful analysis/metering.
