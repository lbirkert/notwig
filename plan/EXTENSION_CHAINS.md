# Device chains for extensions

Status: implemented. Extension devices with `DeviceKind::Container` now get a
host-owned child chain, including a wet path for effects such as Reverb.

Related:

- [`plan/EXTENSION_DEVICES.md`](EXTENSION_DEVICES.md) — candidate devices.
- [`plan/MOVING_DEVICES.md`](MOVING_DEVICES.md) — why `notwig.chain` stays core.
- [`docs/02_extensions/03_device_api.md`](../docs/02_extensions/03_device_api.md)
  — author-facing container contract.
- [`docs/03_development/19_extension_abi_and_hosts.md`](../docs/03_development/19_extension_abi_and_hosts.md)
  — ABI and host boundary.

## What shipped

- `DeviceSpec::container: Option<ContainerSpec>` in `notwig-core`. This is an
  additive field, but it changes the spec wire layout, so `ABI_VERSION` is now
  `5`.
- `ContainerSpec { routing, wet_param }`:
  - `ContainerRouting::StageThenChildren` (default): stage, then child chain.
  - `ContainerRouting::ChildrenThenStage`: child chain, then stage.
  - `wet_param: Option<ParamId>`: host-owned dry/wet blend.
- `ContainerProcessor` in `notwig-core/src/dsp/container.rs` owns the optional
  stage, the serial child chain, latency, nested parameter routing, and the wet
  blend.
- `build_processor` in `notwig-engine` builds a `ContainerProcessor` for any
  non-stock `DeviceKind::Container`. The stock `notwig.chain` keeps its dedicated
  `ChainProcessor`, so existing projects and tests are unchanged.
- `notwig.ext.devices.reverb` is a container using
  `StageThenChildren` with `Mix` as the wet parameter. Its child chain is a wet
  FX chain: nested devices process the reverb wet signal before the host blends
  it with the dry input.

## Signal flow

```text
StageThenChildren, wet_param = Some(mix):
  input -> stage -> child chain -> dry/wet mix -> output

StageThenChildren, wet_param = None:
  input -> stage -> child chain -> output

ChildrenThenStage:
  input -> child chain -> stage -> output
```

`wet_param` is only active when a stage exists. If the extension is missing or
fails to instantiate, the child chain still processes and the dry/wet blend is
skipped, so the container behaves like a plain chain.

## Rules

- The stage's `process` outputs wet only when `wet_param` is set. The host
  applies the mix after the stage and the child chain.
- The wet parameter must be declared in `spec.params`; the host reads its
  `0..=1` value and smooths changes.
- Bypassing the container skips the stage and all children, matching the stock
  chain.
- Latency is stage latency plus the child chain latency. The compile-time plan
  already sums child latency.
- Nested parameter edits route through `set_nested_param` into the child chain.
- Child chains are serial. There is no parallel/split path, sidechain input, or
  guest access to child identity/order.

## Tests added

- `crates/notwig-core/tests/dsp_container.rs`: stage/child order, wet blend,
  missing-stage fallback, nested parameters, latency, unknown device rejection.
- `crates/notwig-engine/tests/render.rs`: a fake extension container processes
  its child chain through the real engine build.
- `crates/notwig-ext/notwig-ext-devices/tests/reverb.rs`: container spec,
  pre-delay, wet tail, reset, and parameter validation.
- `crates/notwig-ext/notwig-ext/tests/limiter_extension.rs`: the built WASM pack
  exposes the Reverb as a container with a wet parameter.

## Remaining work

- Enforce one child chain per Container. The UI expands the first child chain
  while the engine iterates all of them.
- Add maximum nesting depth and cycle detection for child chains. A corrupt
  document can currently recurse in `chain_latency`.
- Optional: expose child count/names to panels (adds `DeviceDrawContext` fields
  and another ABI bump).
- Optional: extension-driven child reorder/mute/add via panel actions.
- Parallel/split routing and sidechain ports need a multi-port DSP ABI.
- A `Rack`/`FX Chain` example device can now be built with a pass-through stage
  and `wet_param = None`; it is not needed for the Reverb use case.
