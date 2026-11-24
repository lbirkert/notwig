# notwig-plugins

Format-agnostic plugin discovery and hosting.

## Purpose

`notwig-plugins` is the only crate that knows about plugin formats. It discovers
and caches plugins, exposes format-agnostic descriptors, loads instances, maps
parameters/state, and manages native editor windows. VST3, VST2, CLAP, LV2, and
AU (v2/v3) hosting uses `notwig-rack`; format-specific types stay inside this
crate.

## Owns

- `PluginHost` and the `HostAdapter` boundary.
- `PluginProbe`: an opt-in crash-isolating metadata probe that runs before any
  plugin code executes in the host process.
- VST3/VST2/CLAP/LV2/AU/AU3 adapters (feature-gated) backed by the vendored
  `notwig-rack` crates under `crates/notwig-rack/`.
- Plugin scan cache read/write. Cache persistence is best-effort and not part of
  project recovery, so it intentionally does not share `notwig-project`'s atomic
  write helper.
- `PluginDescriptor`, its format-prefixed type IDs, `PluginInstance`,
  `PluginState`, capabilities, formats, and `PluginError`.
- Native plugin editor window lifecycle.
- Instantiation, parameter descriptions, state save/restore, and latency
  reporting through the format-agnostic API. When a probe is installed,
  metadata is loaded in a child process first; a crash there only fails the
  plugin instead of the application. The vendored `notwig-rack` wrappers
  query VST3 `getLatencySamples`, VST2 `AEffect::initial_delay`, and AU
  `kAudioUnitProperty_Latency`, so the adapter publishes real latency with
  `reports_latency: true`.

## Does not own

- Plugin scan policy, background process launching, or UI cache display (see
  `notwig-app/src/plugins/` and `notwig-ui/src/views/device_editor/`).
- Device graph placement or real-time execution (see `notwig-core::dsp` and
  `notwig-engine`).
- Project persistence of device instances (see `notwig-core` and
  `notwig-project`).

## Features

| Feature | Meaning |
|---|---|
| `vst3` | VST3 adapter |
| `vst2` | VST 2.4 adapter |
| `clap` | CLAP adapter |
| `lv2` | LV2 adapter (needs system `lilv-0`) |
| `au` | Audio Unit v2 adapter (macOS) |
| `au3` | Audio Unit v3 adapter (macOS) |

Features are opt-in at the crate level. `notwig-app` enables AU, CLAP, and VST
by default on native builds; LV2 is opt-in because it needs the system `lilv`
library. The app can also be built with no format features so integration tests
do not need plugin SDKs.

## Module map

| Module | Contents |
|---|---|
| `host.rs` | `PluginHost`, `HostAdapter`, `PluginProbe`, unavailable adapter |
| `adapters.rs` | VST3/VST2/CLAP/LV2/AU/AU3 implementations |
| `cache.rs` | `PluginCache`, scan records |
| `types.rs` | Format-agnostic descriptors/state/errors |
| `native_editor/` | Native editor windows (facade, editor pump, AppKit host, Space-key monitor, stub) |
| `rack.rs` | `notwig-rack` integration |

## Tests

`tests/` covers cache, descriptor, probe-ordering, and probe-failure behavior.
`tests/host.rs` and `tests/types.rs` run in the normal workspace suite.

`tests/plugin_integration.rs` and `tests/plugin_editor.rs` are manual-only
because they build a real truce plugin from
`tests/fixtures/truce_test_plugin` and load native binaries. Run them through
`scripts/test-plugins.sh`; it builds the available formats, sets `LV2_PATH`,
and invokes the ignored tests. The fixture covers CLAP, VST3, VST2, LV2, AU v2,
and AU v3 where the host platform supports them. Native editor windows are
macOS-only; other platforms assert the backend-unavailable stub.
