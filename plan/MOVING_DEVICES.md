# Moving stock devices to extensions

Status: analysis. Question: should the core stock devices (Tool, EQ, Chain) move
from `notwig-core` into `notwig-ext-devices` or another bundled extension pack?

Short answer: **no for all stock devices.** Keep `Chain`, `Tool`, and `EQ` in the
core as the guaranteed structural/utility minimum. Grow `notwig-ext-devices` as
the bundled creative-device pack. Reconsider individual non-structural devices
only after state/assets and offline-render parity exist. Container support now
ships, but `Chain` remains structural.

Related:

- [`plan/EXTENSION_DEVICES.md`](EXTENSION_DEVICES.md) — candidate bundle devices.
- [`plan/EXTENSION_CHAINS.md`](EXTENSION_CHAINS.md) — container support.
- [`docs/03_development/01_code_structure.md`](../docs/03_development/01_code_structure.md)
  — dependency direction and crate ownership.

## What is stock today

Core devices live in `crates/notwig-core/src/dsp/stock/`:

- `notwig.chain` — `DeviceKind::Container`, nested chains, input gain, mix.
- `notwig.tool` — gain, pan, width, polarity, swap, mix.
- `notwig.eq` — four bands, shift, output gain.

They are compiled into the core registry (`stock_registry`) and instantiated by
`StockDeviceFactory`. `notwig-ui` also has native renderers keyed by their type
ids: `DeviceRendererKind::{Chain, Tool, Eq}` in
`crates/notwig-ui/src/views/device_editor/renderer.rs`.

The graph and engine engine use them as fixtures everywhere:
`crates/notwig-core/tests/dsp_graph.rs`, `crates/notwig-core/benches/dsp.rs`,
`crates/notwig-engine/tests/*`, and the CLI render path in
`crates/notwig-app/src/main.rs:469` all compile against `stock_registry()` and
`StockDeviceFactory`.

Extension devices are already "stock-like": host-loaded specs are merged into
the same `DeviceRegistry` (`crates/notwig-app/src/workspace/devices.rs:160`) and
the app composes `StockDeviceFactory`, the extension factory, and the plugin
factory. So the runtime difference is small; the differences are availability,
dependencies, and structural behavior.

## Arguments for moving devices out of core

- **One device architecture.** Core would stop owning DSP device implementations;
  all devices would use the same factory, spec, panel, and packaging path.
- **Dogfooding.** Every stock device becomes a test of the extension ABI. Gaps
  like missing state, analysis data, and containers become visible immediately.
- **Faster iteration.** Creative devices could be updated with the bundle instead
  of a core release.
- **Smaller core.** `notwig-core` stays closer to the document model and DSP
  graph, with fewer stock-specific concerns.
- **Uniform UI.** Native renderer special cases disappear; all stock devices use
  the extension panel path.

## Arguments against

- **Bootstrap and availability.** A fresh install with a disabled, moved, or
  broken extension pack would have no EQ, Tool, or Chain. Core devices must
  always resolve, including in offline CLI render and recovery scenarios. The
  bundled stock pack is currently auto-installed only by the browser extension
  manager (`crates/notwig-app/src/extensions/web.rs:30`); native installs still
  require installing the ZIP. Moving stock devices would make that gap a
  missing-device outage.
- **`Chain` is structural, not a leaf device.** The engine special-cases
  `notwig.chain` when building the graph
  (`crates/notwig-engine/src/runtime/graph.rs:86`), and the model gives every
  Container a child chain (`crates/notwig-app/src/workspace/mod.rs:1316`).
  Moving it to an extension would either keep a hardcoded host container anyway
  or require extension containers first. It is the wrong first candidate.
- **Offline render and CLI.** `render_doc` compiles with `stock_registry()` and
  a factory with no extension host (`crates/notwig-app/src/main.rs:469-495`).
  Extension devices in a project already render as unresolved there. Moving
  stock devices would move Tool/EQ/Chain into that unresolved bucket unless the
  CLI learns to load the bundled pack. That is a real regression for render,
  stems, and automation-free command-line workflows.
- **Core test and benchmark fixtures.** Dozens of tests build graphs with
  `stock_registry()` and `StockDeviceFactory`. Moving devices means either a
  test-only stock pack, an embedded extension in tests, or mass rewrites. The
  lowest-layer tests would start depending on the extension host.
- **Performance and real-time shape.** Tool and EQ are common on many tracks.
  Core processors run directly. WASM devices add per-block copies, bounds
  checks, and sandbox overhead; native dylibs are platform-specific and require
  per-arch builds. That trade may be fine for a limiter but is harder to justify
  for a per-channel EQ. Benchmark before moving a device that runs everywhere.
- **ABI as a dependency for the most basic devices.** Every ABI bump would force
  a bundled-pack rebuild and create a mismatch window between app and pack. Core
  devices never have that failure mode. A bundled pack must be pinned to the app
  and effectively non-removable, which weakens the "extension" story anyway.
- **Panic and debugging behavior.** A panic in a core device is a normal Rust
  panic with a backtrace. A panic in a WASM device traps the instance; in a
  native extension it aborts the process
  (`docs/03_development/19_extension_abi_and_hosts.md`). Basic mixing devices
  should not inherit that failure mode.
- **UI parity and persistence.** Moving a device means reimplementing its panel,
  keeping the exact type id and parameter ids, and proving old projects load.
  `notwig.eq` and `notwig.tool` are currently native egui panels with specific
  visuals; parity is work with no user-facing gain.

## Per-device recommendation

| Device | Move to extension? | Reason |
| --- | --- | --- |
| `notwig.chain` | No, permanently | Structural container primitive. Host owns nesting, latency, nested params, and bypass. See [`plan/EXTENSION_CHAINS.md`](EXTENSION_CHAINS.md). |
| `notwig.tool` | No | Small, ubiquitous utility. Cheap, guaranteed, latency-critical, zero dependencies. |
| `notwig.eq` | No for now | Ubiquitous channel EQ with a native renderer and many parameters. Revisit only if extension EQ becomes clearly better (dynamic bands, spectrum, state). |
| Limiter, Gate, Synth, Gain | Already extensions | Creative devices with panels; good fit for the bundled pack. |
| Future creative devices | Yes | Reverb, delay, saturation, chorus, drum synth, etc. belong in the bundle. |

The split that makes sense is **core structural/utility minimum plus a bundled
creative-device extension pack**, not "all devices are extensions."

## If a device is moved later

Preconditions and rules to make a migration safe:

1. **Keep `Chain` core even now that extension containers exist.** The stock
   chain is the structural primitive the engine and UI rely on.
2. **Keep the exact type id and parameter UUIDs.** Existing projects store
   strings and ids; changing them marks devices unresolved. The extension may
   still live in a package whose id uses the `notwig.ext.*` convention.
3. **Ship the pack as a bundled, pinned, enabled-by-default package.** Prefer
   embedding it in the app or installing it at first run, not asking the user to
   install it. Treat removal/disable as an advanced action with a clear warning.
4. **Teach the CLI render path to load the bundled pack.** Otherwise offline
   render silently loses those devices. Add a render test for a project that
   uses them.
5. **Keep a core fallback or a clean error.** If the pack fails to load, the
   device must produce a visible unresolved/broken state, not silence with no
   explanation.
6. **Benchmark real projects.** Compare CPU and latency for a per-channel EQ on
   tens of tracks, WASM versus native dylib, before committing.
7. **Prove parity.** Port the panel, keep the parameter behavior, run null tests
   against the core implementation, and add round-trip project tests with old
   files.
8. **Move one device at a time.** Delete the native renderer branch and the DSP
   implementation only after the extension path has shipped and survived a
   release.

## Consequences for `notwig-ext-devices`

Until the ABI grows state/assets, analysis data, transport, and note output, the
bundled pack can cover parameter-only DSP devices and serial containers. That is
exactly the profile of the candidate creative devices in
[`plan/EXTENSION_DEVICES.md`](EXTENSION_DEVICES.md). Keep Tool/EQ/Chain core and
let the pack be the showcase of what extensions can do well, rather than forcing
the extension system to carry the minimum viable mixer.
