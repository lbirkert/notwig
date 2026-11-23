# Routing and delay compensation

Status: implemented in the domain and engine; routing controls are not exposed in
the UI. This page describes how tracks reach the master, how sends and nested
groups are mixed, how device latency is aligned, and what the current limits are.

Related: [`Audio Engine`](02_audio_engine.md) for the control exchange and
monitoring path, [`DSP Implementation and Performance`](08_dsp_performance.md)
for device internals, and [`Code Structure`](01_code_structure.md) for the
module map.

## 1. Routing model

A project is a forest of tracks that ultimately feeds one `Master` track.
`TrackKind` has five values: `Audio`, `Instrument`, `Group`, `BusFx`, and
`Master`. Audio and instrument tracks own clips; group tracks own children; the
master is the final sink and cannot route onward.

Each track stores a main output in `TrackIO::audio_out` (`TrackOutput`):

| `TrackOutput` | Destination |
| --- | --- |
| `Default` | The track's `parent` when it has one, otherwise the master (except the master itself). |
| `Track(id)` | An explicit internal track target. |
| `None` | No main output. Used by the master and available for sink-like tracks. |

Group children use `Default` and therefore feed their parent group. The plan
keeps the parent edge separately from the explicit output, so a depth-first
track order, the parent tree, and an explicit target can coexist.

Sends are stored per track as `Send { id, target, amount_db, pre_fader,
enabled }`. A send copies one track's signal into another track's input, scaled
by `amount_db`, without replacing the main route. Pre-fader sends tap the track
input (after the input alignment delay, before its device chain and fader);
post-fader sends tap the processed, gain/pan-applied track output.

The engine builds a processing order from both main routes and enabled sends.
It emits `EngineError::Cycle` rather than rendering a cycle. Disabled and
self-targeting sends are ignored.

```text
                       ┌──────────────────────┐
                       │        Master        │──► device output
                       │ devices → fader      │
                       └───────▲───────▲──────┘
                               │       │ main route (Default)
              ┌────────────────┴───┐   │
              │ Group (devices,    │───┘
              │ fader, parent)     │
              └────────▲───────────┘
                       │ parent (Default)
        ┌──────────────┴───────────────┐        ┌──────────────────┐
        │ Audio / Instrument track     │───────►│ BusFx / group     │
        │ clip/input → devices → fader │  send  │ (send target)     │
        └──────────────────────────────┘        └────────┬─────────┘
                                                          │ main route
                                                          ▼
                                                       Master
```

## 2. Per-track signal flow

The runtime keeps one buffer per track plus a pre-fader copy. Both are
preallocated at `max_block_size`.

```text
 clip material / live monitor input
        │
        ▼
 input_delay ── aligns clip material and inputs to the track's input label
        │
        ├──────────────► pre-fader send tap ──────────────┐
        ▼                                                 │
 device chain (main_chain, nested Chain containers)       │
        │   inactive devices are not built                 │
        ▼                                                 │
 track gain + pan (post-fader)                            │
        │                                                 │
        ├──────────────► post-fader send tap ────────────┤
        ▼                                                 │
 main_delay ── pads this connection to the target label   │
        │                                                 │
        ▼                                                 ▼
 parent / master / explicit target                  send_delays → send target
```

Gain and pan are applied after devices. The master track is not panned. Metering
runs after routing on the post-gain buffer.

## 3. Bypass, deactivation, and inactive tracks

Several switches remove processing, and they are not the same:

| State | Device processing | Latency contribution | Routing |
| --- | --- | --- | --- |
| Device `bypass` | Processor and plugin stay built and loaded, but DSP is skipped. | Device latency stays in the plan; the bypass wrapper delays the dry signal by that latency. | The dry signal passes through the remaining chain aligned with other paths. |
| Device `active = false` | Processor is not built for that device. | Device latency becomes `0`; the chain sums only active devices. | The dry signal passes through the remaining chain; a structural rebuild unloads the instance. |
| Track `active = false` | No main-chain processors are built for the track and the render loop skips its device chain. | Track latency becomes `0`; the plan recomputes labels without it. | The track still reaches its main route and enabled sends; gain, pan, mute, and solo apply on the main and post-fader paths. |
| Track `mute` | Devices still process. | Unchanged. | Main and post-fader sends carry silence; pre-fader sends keep the input tap. |
| Track not soloed | Devices still process. | Unchanged. | Main and post-fader sends are silenced by the effective solo state. |

An inactive track is therefore an unprocessed pass-through at zero latency, not
a muted track. Re-enabling it rebuilds the plan so its devices and latency
rejoin the compensation. Device deactivation is also structural and removes the
processor from the runtime. Device bypass is non-structural: the engine receives
a runtime control update, keeps the loaded instance, and switches the wrapper to
a delayed dry path. Nested `Chain` devices apply their own child rules
recursively; `ContainerProcessor::latency` and `ChainProcessor::latency` sum only
the children that exist.

## 4. Latency labels and delay lines

Graph compilation assigns four latency fields to every `PlanTrack`:

| Field | Meaning |
| --- | --- |
| `latency_samples` | Sum of active device latency in the track's main chain, including nested chains. Bypassed devices keep their latency. |
| `input_latency` | The latest arrival label among all main-routed and enabled send inputs; clip/input material starts at label `0`. |
| `output_latency` | `input_latency + latency_samples`. |
| `latency_to_master` | `total_latency - input_latency`; the maximum over all tracks equals the plan's `total_latency`. |

`compute_latency_labels` processes tracks in dependency order. For each
connection it records the source label: `output_latency` for a main route and a
post-fader send, or `input_latency` for a pre-fader send. The destination's
`input_latency` becomes the maximum of those labels. The master's
`output_latency` is `CompiledPlan::total_latency`.

Each `PlanSend` then stores its own `latency_samples = target.input_latency -
source_label`, and the runtime creates one delay line per send. Main connections
store the same difference as `main_delay` on the source track. Clip material and
live input are delayed through `input_delay` by the track's `input_latency`.
Because every node delays only the difference between its destination label and
its own label, a path is never compensated twice and nested groups stay coherent.

```text
 source output label 100 ──► target input label 160
                                │
 target.input_latency (160)     │  main_delay = 60
 source.output_latency (100)    ▼
 source buffer ── DelayLine(60) ──► target buffer
```

Delay lines are `Vec<f32>` owned by `TrackRuntime` and sized when the runtime is
built. They never allocate during rendering. A plan with no latency uses
zero-length delay lines, which copy through immediately.

The plan's total latency is published as `RtEvent::LatencyChanged { device_id:
None, samples }` when a runtime is installed. It is sample-based plan latency, not
device/driver latency.

## 5. Plugin latency reporting

Hosted plugin latency flows through three stages:

1. `notwig-plugins` queries the wrapper and fills `PluginCapabilities`.
   `PluginInstance::capabilities` reports `latency_samples` for the instance.
2. `notwig-app::Workspace` resolves the plugin metadata and copies
   `capabilities.latency_samples` into the plugin's `DeviceSpec`.
3. `notwig-core::dsp::compile` copies `DeviceSpec::latency_samples` into
   `PlanDevice`, then `chain_latency` sums it into `PlanTrack.latency_samples`.
   Inactive devices contribute zero; bypassed devices keep their latency.

The vendored wrappers implement `PluginCore::latency_samples` for the formats
that expose a query:

| Format | Source | Reported latency |
| --- | --- | --- |
| VST3 | `getLatencySamples` | Plugin-reported samples. |
| VST2 | `AEffect::initial_delay` | Plugin-reported samples, clamped to `u32`. |
| AU v2 | `kAudioUnitProperty_Latency` | Plugin-reported samples, rounded. |
| CLAP | No latency query in the wrapper | `0`. |
| LV2 | No latency query in the wrapper | `0`. |

The default implementation returns `0`, so a format without a query is treated
as zero-latency rather than guessed. Extension devices report
`DeviceSpec::latency_samples`; their processor may also expose a runtime
`Latency`, but the plan uses the declared spec value at build time. A latency
change that is not reflected in the spec requires a structural rebuild to affect
PDC.

## 6. Send vs direct phase alignment

A track can reach a destination twice at once: through its main route (possibly
via groups) and through a send. Without alignment, the two copies would arrive at
different times because the direct path includes downstream device latency that
the send path may skip, and pre-fader taps sit before the source chain.

The plan resolves this per connection:

- The send's source label is the source's `input_latency` for a pre-fader send
  and `output_latency` for a post-fader send.
- The send delay is `target.input_latency - source_label`.
- The main connection delay is `target.input_latency - source.output_latency`.
- Clip material is delayed by the source's `input_latency`.

All copies therefore enter the destination's input at the destination's input
label. `tests/pdc.rs` covers a send arriving at the same sample as the direct
path, along with flat latent tracks, nested group latency, and bypassed devices.

## 7. Known limits

| Limit | Effect |
| --- | --- |
| No routing UI | The document and engine support `TrackOutput::Track`, groups, and sends, but no normal UI action changes a main route or edits a send. |
| No send/return workflow | Sends can be stored and executed, but there is no UI to create return tracks, set send amounts, or choose pre/post fader. |
| No sidechains or multi-out | Plugins receive a single stereo input and stereo output. |
| CLAP/LV2 latency is `0` | These formats are not delay-compensated unless an adapter learns to query latency. |
| Latency changes need a rebuild | Plan labels are computed at compile time; a host that changes latency without a spec change is not re-aligned mid-playback. |
| No lookahead/negative delay | PDC can only delay earlier paths; a connection cannot pull a path earlier than `0`. All labels are saturating `u32`. |
| Track inactive is not mute | An inactive track still routes unprocessed audio to its output; use mute to silence it. |
| PDC is sample-based | It compensates plan/device latency only. Driver, converter, and monitoring latency are separate (see [`Audio Engine` §9](02_audio_engine.md#9-monitoring-latency-and-recording-capture)). |

## 8. Tests

- `crates/notwig-engine/tests/pdc.rs` — direct vs latent tracks, nested group
  compensation, and send/direct phase alignment.
- `crates/notwig-engine/tests/render.rs` — routed rendering, mute/solo, stop
  tail, and offline parity.
- `crates/notwig-core/tests/dsp_graph.rs` — plan compilation, latency labels,
  bypass, nested-chain, and inactive-track latency.
- `crates/notwig-core/tests/domain_flow.rs` — routing cycle rejection at the
  document boundary.

## 9. Where to make changes

- Routing fields and validation: `crates/notwig-core/src/tracks.rs`.
- Latency label computation and plan fields: `crates/notwig-core/src/dsp/graph.rs`.
- Processing order and routing targets: `crates/notwig-engine/src/runtime/graph.rs`.
- Mixing and delay lines: `crates/notwig-engine/src/runtime/routing.rs`.
- Runtime state and delay-line construction: `crates/notwig-engine/src/runtime.rs`.
- Plugin latency querying: `crates/notwig-plugins/src/rack.rs` and the vendored
  `notwig-rack-*` wrappers.
