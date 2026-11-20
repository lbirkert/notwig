# notwig-rack-vst2

VST 2.4 host implementation for the [**notwig-rack**](../README.md) audio
plugin framework. This crate is new in the fork; upstream truce-rack does not
host VST2.

It uses a clean-room VST 2 ABI translated from Xaymar/vst2sdk (BSD-3-Clause)
and `libloading`, with no Steinberg SDK and no C++ glue. It implements scan,
load, audio processing, MIDI and SysEx I/O, parameters, programs, opaque chunk
and parameter state, host transport (`audioMasterGetTime`), plugin latency
(`AEffect::initial_delay`), and VST2 editors. Shell plugins are enumerated
through `effShellGetNextPlugin`.

VST2 only defines 32-bit float audio, so the wrapper implements
`Plugin<f32>`. Both `processReplacing` and the legacy accumulating `process`
callback are supported.

## Usage

```toml
[dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-vst2 = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_vst2::Vst2Scanner;

let plugins = Vst2Scanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option. The VST2 ABI translation is BSD-3-Clause;
see the header in `src/abi.rs` and the repository `NOTICE` file.
