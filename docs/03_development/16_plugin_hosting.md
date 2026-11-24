# Plugin Hosting

`notwig-plugins` is the only crate that knows plugin formats. It owns the
format-agnostic host/adapter contracts, discovery and cache, metadata probing,
instances, state, and native editor windows. `notwig-engine` bridges instances
into the device graph. Native desktop builds compile VST3, VST2, CLAP, LV2, AU,
and AU3, though AU/AU3 register only on Apple; the browser has an empty catalog.

Related: [`Audio Engine`](02_audio_engine.md),
[`Engine Process and Supervision`](15_engine_process_and_supervision.md), and
[`Crate Reference`](07_crate_reference.md).

## 1. Contracts

| Contract | Implemented by | Responsibility |
| --- | --- | --- |
| `HostAdapter` | VST3/VST2/CLAP/LV2/AU/AU3 adapters, `UnavailableAdapter` | Declares a format; `scan` returns descriptors; `load` returns an instance; `open_editor` defaults to `BackendUnavailable`. |
| `PluginHost` | `PluginHost` | Holds adapters in registration order, the scan cache, an optional probe, and probed metadata. Resolves an adapter by format and gates loads on blacklist/feature state. |
| `PluginProbe` | App `SubprocessProbe` | Runs plugin metadata loading outside the host process; returns serializable `PluginMetadata`. |
| `PluginInstance` | `RackPluginInstance` | Extends `DeviceProcessor` with capabilities, state save/load, and parameter descriptions. |
| `DeviceProcessor` | `RackPluginInstance`, stock devices | `prepare`, `reset`, `set_param`, `process`, `latency`; the engine calls these from its build and audio paths. |
| `PluginDeviceFactory` | `notwig-engine` | Selects stock processors first, else loads a descriptor through `PluginHost`, restores state, and wraps the instance for graph construction. |

## 2. Discovery, cache, and probing

`PluginCache` is a JSON map of `ScanRecord { descriptor, blacklisted, failures,
last_error }`. Three recorded load/probe failures blacklist a type ID; scans skip
blacklisted entries. Cache persistence is best effort and is not part of project
recovery. `PluginHost::scan` walks each adapter with the supplied paths, records
descriptors, and sorts by format, path, then UID.

The app's `PluginCatalog` loads the cached list synchronously, then spawns a child
process running the same executable with `scan-plugins <cache-path>`. That child
constructs a `PluginHost`, scans `plugin_scan_paths`, and writes the cache; the UI
swaps in the list when the child finishes. Scan roots are macOS locations
(`~/Library/Audio/Plug-Ins/...` and `/Library/Audio/Plug-Ins/...`).

```text
UI PluginCatalog                         child processes
  load cache JSON
  spawn scan-plugins ──────────────────► PluginHost::scan -> adapters
  poll descriptor list ◄──────────────── cache JSON written
  ensure_plugin_spec
  probe_metadata ──────────────────────► load-plugin (descriptor JSON on stdin)
                                           load plugin, print metadata line
  metadata ◄──────────────────────────── NOTWIG_PLUGIN_METADATA:{...}
```

The UI-side host installs `SubprocessProbe`, which caches by format, type ID,
path, modification time, and size. It starts `load-plugin` with a 30 s timeout and
parses the last `NOTWIG_PLUGIN_METADATA:` line. A child crash fails that plugin,
not the UI. `PluginHost::load` probes first when a probe is installed. The engine
process constructs `PluginHost::new()` without a probe, so its DSP loads are not
crash-isolated from the engine process.

## 3. Scan and load threading

Discovery runs in a `scan-plugins` child; metadata probing runs in a `load-plugin`
child; plan compilation runs on the UI's `notwig-engine-build` thread; plugin
instantiation and preparation run on the engine child's `notwig-engine-loader`
thread. Only plugin `process` runs on the audio path. The engine factory owns a
`Mutex<PluginHost>` used solely while building processors.

## 4. Load, prepare, process, state, parameters, latency

`PluginDeviceFactory::create` first asks `StockDeviceFactory`, then looks up the
descriptor by type ID, calls `PluginHost::load`, restores the saved `PluginState`
if present, optionally attaches a monitor for an open editor, and returns a
`PluginDevice`. A missing descriptor or failed load returns `None`; graph building
then omits that processor without failing the build, so the chain passes audio
through silently.

`RackPluginInstance` maps the format-agnostic call to a `notwig_rack::Plugin<f32>`.
It uses a stereo contract: `prepare` deactivates, chooses a 2-in/2-out layout,
activates at the configured sample rate and block size, and sizes per-channel
scratch buffers. `process` copies interleaved stereo into channel buffers, converts
note on/off events to channel-0 MIDI, supplies transport, calls the plugin, and
copies the output back. A plugin error or undersized scratch yields silence with
`overloaded: true`; `Sleep` yields `produced_silence: true`. MIDI output and
non-stereo buses are not surfaced.

State is a `PluginState { format, plugin_uid, plugin_version, blob }`.
`load_state` rejects a format/UID mismatch but treats version as informational.
The factory ignores restore errors. Parameter IDs are the plugin's `u32` ID widened
into a `ParamId` UUID; `set_param` only applies `f64` values and ignores plugin
errors. `describe_parameters` maps rack info into `ParamSpec`. Latency comes from
`DeviceProcessor::latency`: VST3 `getLatencySamples`, VST2 `initial_delay`, and the
AU latency property; CLAP and LV2 leave the default `0`.

## 5. Editor hosting

Native editors live in the engine child, not the UI process. The adapter
`open_editor` hook is unused; `NativePluginWindow` loads the plugin directly. Each
editor is a separate plugin instance from the DSP instance, so state crosses the
boundary only at open/close and through parameter messages.

```text
UI process                               engine-host child (main thread)
open_plugin_window
  OpenEditor(descriptor,state,params) ──► EditorHost::open
                                           NativePluginWindow::open
                                           load_state + set_parameter
  EditorOpened ◄───────────────────────────
  SetParam / Focus / Close ───────────────► set_parameter / focus / save_state
  EditorParam (50 ms poll) ◄─────────────── poll_changes
  EditorClosed { state } ◄────────────────── save_state on close
  store_plugin_state -> state/<device>.json
```

The UI applies an `EditorParam` as a document `SetParam`, publishes it to the DSP,
and mirrors UI-originated changes back with `SetEditorParam`. A monitor tap feeds
the editor instance so plugin meters and visualizers receive audio. Closing saves
state into the workspace and marks the project dirty; the app shutdown path waits
up to two seconds for these close events. Native windows exist only on macOS; the
non-macOS stub returns `BackendUnavailable`. A crash loses live editor state that
was not previously closed.

## 6. Format adapters

| Format | Build gate | Discovery | Latency | State | Notes |
| --- | --- | --- | --- | --- | --- |
| VST3 | feature `vst3` | filesystem paths | `getLatencySamples` | yes | Editor supported. |
| VST2 | feature `vst2` | filesystem paths | `initial_delay` | chunk or parameter dump | Editor when flagged. |
| CLAP | feature `clap` | filesystem paths | not wired, reports `0` | `clap.state` | Editor via GUI extension. |
| LV2 | feature `lv2` | registry (paths ignored) | not wired, reports `0` | save/load return "not yet wired" | No parameter enumeration/set. |
| AU | feature `au` + Apple target | registry (paths ignored) | AU latency property | yes | macOS editor. |
| AU3 | feature `au3` + Apple target | registry (paths ignored) | delegates to AU v2 | yes | Load reuses the AU v2 loader. |

`notwig-plugins` has no default format features; the native app enables all six.
Adapter registration is feature-gated, and AU/AU3 are additionally gated to Apple
targets. Format enablement in `PluginFormat::enabled` is compiled-feature based.

## 7. Real-time constraints and gaps

- CLAP and LV2 reuse event/pointer scratch buffers; VST3 still allocates its
  per-block event-list storage, and hosted plugins may allocate internally. The
  engine graph is allocation-free, but plugin processing is not guaranteed to be.
- Only stereo I/O is used. Sidechains, multi-bus layouts, MIDI channels other than
  0, and plugin-produced notes are not handled.
- Load failures in `PluginDeviceFactory::create` become an absent processor rather
  than an error. State-restore and parameter-set failures are also ignored.
  Low-latency monitor prepare failures are now reported as `RtEvent::Error`.
- Adapters zero output when a plugin reports a process error. The core status type
  cannot distinguish `Error` from overload, so the engine reports a device fault
  and sanitizes output without naming the plugin.
- `RackPluginInstance` reports `supports_state: true` and `reports_latency: true`
  for every format, although LV2 state is unwired and CLAP/LV2 latency is `0`.
  Latency compensation therefore trusts a zero for those formats.
- The engine process uses no subprocess probe. A plugin that hard-crashes during
  DSP load or process takes down `engine-host`; the UI survives and can restart it.

## 8. Integration fixture and manual matrix

`crates/notwig-plugins/tests/fixtures/truce_test_plugin` is a local truce
plugin with three parameters (gain, bypass, mode) and an editor. It is built
outside the Notwig workspace by `scripts/test-plugins.sh`, which selects the
formats available on the host, sets `LV2_PATH` to a dedicated directory, and
runs the ignored integration tests.

| Test file | Covers |
| --- | --- |
| `tests/plugin_integration.rs` | Per-format discovery, descriptor shape, load, capabilities, editor capability, latency expectations, parameter updates, state round-trip, wrong-UID rejection, corrupt-state safety, re-prepare at another rate/block size, zero-frame and silent blocks, and repeated instantiation. |
| `tests/plugin_editor.rs` | macOS `NativePluginWindow` open/size/pump/focus, host-to-editor parameter change, editor state save/load, and reopen. Off macOS it asserts `BackendUnavailable`. |

The format matrix is intentionally explicit about adapter gaps:

| Format | Host test | Parameters | State | Editor |
| --- | --- | --- | --- | --- |
| CLAP | yes | native units | yes | macOS only |
| VST3 | yes | normalized `[0, 1]` | yes | macOS only |
| VST2 | yes | normalized `[0, 1]` | yes | macOS only |
| LV2 | yes (needs lilv + `LV2_PATH`) | not enumerated | not wired | macOS only |
| AU v2 | macOS, component must be installed | native units | yes | macOS only |
| AU v3 | macOS, extension must be installed | native units | yes | macOS only |

VST3 and VST2 currently expose normalized parameter values while CLAP and AU
expose native units. The tests translate to each adapter's wire form instead
of hiding that difference. Host `EnumIndex` values are not yet mapped through
normalized adapters' step counts, so enum coverage uses the raw wire value.

Run the matrix manually:

```sh
cargo install cargo-truce --version 6.3.0
scripts/test-plugins.sh
NOTWIG_PLUGIN_TEST_AU3=1 scripts/test-plugins.sh   # macOS AU v3 as well
```
