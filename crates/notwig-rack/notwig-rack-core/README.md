# notwig-rack-core

Core traits and types for the [**notwig-rack**](../README.md) audio plugin host
framework, forked from [truce-rack](https://github.com/truce-audio/truce-rack).

The crate defines the format-agnostic `PluginScanner`, `PluginCore`,
`Plugin<S>`, and `PluginEditor` traits every per-format wrapper adapts plugins
into. It has no FFI and no platform-specific dependencies.

`ProcessContext` carries the optional `TransportInfo` host snapshot (tempo,
time signature, song position, bar start, play/record/loop state). The
`PluginCore::latency_samples` method reports processing latency for delay
compensation; it defaults to zero for formats that cannot query it.
`PluginInfo::supports_ara` reports whether a scanner found ARA capability; ARA
binding and the document runtime live in `notwig-rack-ara`.

## Companion crates

- [`notwig-rack-clap`](../notwig-rack-clap) — CLAP
- [`notwig-rack-vst3`](../notwig-rack-vst3) — VST3
- [`notwig-rack-vst2`](../notwig-rack-vst2) — VST 2.4
- [`notwig-rack-au`](../notwig-rack-au) — Audio Unit v2 (Apple)

## License

MIT or Apache-2.0, at your option.
