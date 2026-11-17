# Device UI (`notwig-ui`)

## 1. Presentation
`DeviceView(d) = Header + Body(ParamBindings) + Viz?`
- Common chrome: `DeviceHeader`, chain slot, bypass, status/latency, actions.
- Bodies compose shared primitives; parameter semantics uniform.

## 2. Parameter Mapping
- Visible params bound via `ParamSpec` (labels/units/ranges/enums/`normMap`).
- Read from `S_doc` device state, write `Command::SetParam{deviceId,paramId,value}` (and related device commands).
- Unsupported/plugin-specific features: generic parameter list and/or hosted native editor; core behavior consistent.

## 3. Stock Devices (v0)
- No stock instrument UI exists (no stock instrument in v0).
- **Tool (`notwig.tool`)**: gain/pan-balance (constant-power)/width/polarity/dry-wet controls + input/output meters from `S_rt`.
- **EQ (`notwig.eq`)**: interactive `EqCurve` nodes mapped to band params, band list/precision controls, pre/post `Spectrum` overlay from RT analysis stream; all views consistent with the same params.
- **Chain (`notwig.chain`)**: container gain/dry-wet, nested `DeviceContainer` with structural edits, aggregate latency/status display.

## 4. Hosted Instruments & Plugins (VST3/AU primary, VST2 fallback)
- Instrument tracks show the hosted instrument as a regular device slot (one instrument per Instrument track).
- Generic param view from discovered `ParamSpec` when available.
- Native editors are floating OS windows in v0, opened/closed via non-RT requests and pumped by the UI/main thread; failures are isolated with recoverable UI status.
- State/preset operations surfaced as commands/async results consistent with host contracts; notes-only event flow means no CC/pitch-bend/program-change controls in v0.

## 5. Visualization Data
- Meters/spectrum/analysis/waveform data delivered as `S_rt` or async asset-derived buffers, decimated to view; no RT allocations or UI blocking.
