# notwig-rack-vst3

VST3 host implementation for the [**notwig-rack**](../README.md) audio plugin
framework, forked from [truce-rack](https://github.com/truce-audio/truce-rack).
Built on the community [`vst3`](https://crates.io/crates/vst3) crate — no
Steinberg SDK submodule and no cmake.

Implements scan, load, audio and MIDI processing, state, parameters, latency
reporting (`IAudioProcessor::getLatencySamples`), and the VST3 `IPlugView` GUI.
Host transport from `ProcessContext::transport` is translated into Steinberg's
`Vst::ProcessContext`.

## Usage

```toml
[dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-vst3 = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_vst3::Vst3Scanner;

let plugins = Vst3Scanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option.
