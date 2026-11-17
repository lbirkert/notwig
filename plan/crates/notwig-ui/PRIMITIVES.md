# UI Primitives (`notwig-ui`)

## 1. Design System
- Shared primitives for controls/visualizers with consistent tokens, states (hover/active/focus/disabled), and hit-testing.
- Stock and hosted-plugin UIs compose primitives; avoid bespoke per-device drawing except via primitive extensions.

## 2. Catalog (v0)
| Primitive | Input/Binding | Contract |
|---|---|---|
| `Knob` | value + `ParamSpec` | drag/scroll/wheel, fine adjust, bipolar/unipolar, readout/tooltip |
| `Slider` | value H/V | linear drag, norm/phys mapping, optional detent |
| `Toggle/Button` | bool/action/enum | click/press, latched/momentary |
| `Meter` | `S_rt` levels | peak/rms, decay/ballistics, clip indicator, no per-frame alloc |
| `Waveform` | asset peaks/LOD + clip | viewport LOD render, trim/fade/gain overlays |
| `Spectrum` | `S_rt` analysis frames | streamed, pre/post, stable scaling |
| `EqCurve` | band params + response | interactive nodes/handles -> param commands |
| `DeviceHeader` | device meta/state | name, bypass, status/latency, actions |
| `DeviceContainer` | chain | ordered slots, insert/reorder/remove visuals |
| `Panel` | content | titled/collapsible/scrollable |
| `SplitView` | pane sizes (`S_ui`) | resizable, min sizes |
| `Menu/ContextMenu` | actions | command dispatch, keyboard nav |
| `TimelineRuler/Grid` | tempo map + viewport | musical/time ticks (constant map v0) |

## 3. Parameter Binding
- `B(p): read(S_doc/device) -> write(Command::SetParam/...)` using `ParamSpec.normMap` only.
- Continuous drags may throttle/preview; committed semantic values emitted as commands and reflected from `S_doc`.
- Disabled/unavailable params rendered from capability/flags.

## 4. Invariants
- Deterministic hit targets/z-order, HiDPI legibility, accessible labels from specs/names where possible.
- Dynamic visuals tolerate dropped frames without corrupting interaction state.
