# notwig-rack-vst3

VST3 host implementation for the [**notwig-rack**](../README.md) audio plugin
framework, forked from [truce-rack](https://github.com/truce-audio/truce-rack).
Built on the community [`vst3`](https://crates.io/crates/vst3) crate — no
Steinberg SDK submodule and no cmake.

Implements scan, load, audio and MIDI processing, state, parameters, latency
reporting (`IAudioProcessor::getLatencySamples`), and the VST3 `IPlugView` GUI.
Host transport from `ProcessContext::transport` is translated into Steinberg's
`Vst::ProcessContext`. Parameter updates are sent both to the edit controller
and to the audio processor through `ProcessData::inputParameterChanges`, which
is required when a plugin uses a separate controller class.

With the optional `ara` feature, scan marks audio classes that have a matching
`ARA Main Factory Class`, and `Vst3Plugin` implements `notwig_rack_ara::AraPlugin`.
The ARA 1 `ARAVST3::IPlugInEntryPoint` and ARA 2 `IPlugInEntryPoint2` COM
interfaces are declared in pure Rust, so no Steinberg C++ SDK is required.

## Usage

```toml
[dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-vst3 = { path = ".", features = ["ara"] }
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
