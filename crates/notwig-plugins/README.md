# notwig-plugins

Format-agnostic plugin discovery and hosting.

## Purpose

`notwig-plugins` is the only crate that knows about plugin formats. It discovers
and caches plugins, exposes format-agnostic descriptors, loads instances, maps
parameters/state, and manages native editor windows. VST3 and AU hosting on
macOS uses `truce-rack`; format-specific types stay inside this crate.

## Owns

- `PluginHost` and the `HostAdapter` boundary.
- VST3/AU adapters (feature-gated) backed by the vendored `notwig-truce-rack`
  crates under `crates/notwig-truce-rack/`.
- Plugin scan cache read/write. Cache persistence is best-effort and not part of
  project recovery, so it intentionally does not share `notwig-project`'s atomic
  write helper.
- `PluginDescriptor`, its format-prefixed type IDs, `PluginInstance`,
  `PluginState`, capabilities, formats, and `PluginError`.
- Native plugin editor window lifecycle.
- Instantiation, parameter descriptions, state save/restore, and latency
  reporting through the format-agnostic API. The vendored `truce-rack` wrappers
  query VST3 `getLatencySamples` and AU `kAudioUnitProperty_Latency`, so the
  adapter publishes real latency with `reports_latency: true`.

## Does not own

- Plugin scan policy, background process launching, or UI cache display (see
  `notwig-app/src/plugins.rs` and `notwig-ui/src/views/device_editor/`).
- Device graph placement or real-time execution (see `notwig-dsp` and
  `notwig-engine`).
- Project persistence of device instances (see `notwig-core` and
  `notwig-project`).

## Features

| Feature | Meaning |
|---|---|
| `vst3` | VST3 adapter |
| `au` | Audio Unit adapter (macOS) |
| `vst2` | Reserved format; no host adapter is currently implemented |

Default features are `vst3` and `au`.

## Module map

| Module | Contents |
|---|---|
| `host.rs` | `PluginHost`, `HostAdapter`, unavailable adapter |
| `adapters.rs` | VST3/AU implementations |
| `cache.rs` | `PluginCache`, scan records |
| `types.rs` | Format-agnostic descriptors/state/errors |
| `native_editor.rs` | Native editor windows |
| `rack.rs` | `truce-rack` integration |

## Tests

`tests/` covers cache and descriptor behavior. Plugin loading is platform- and
installation-dependent; avoid tests that require a specific third-party plugin.
