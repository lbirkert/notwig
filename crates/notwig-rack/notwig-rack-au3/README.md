# notwig-rack-au3

Audio Unit v3 (App Extension) host implementation for the
[**notwig-rack**](../README.md) audio plugin framework, forked from
[truce-rack](https://github.com/truce-audio/truce-rack). Apple-only.

Filters the registry walk to v3-flagged components and forwards loading to
`notwig-rack-au`, which handles both synchronous and asynchronous
(`AudioComponentInstantiate` with a completion block) instantiation paths. Once
the instance handle is in hand the audio + MIDI + editor surface is identical to
AUv2 — including host transport, which flows through the same
`kAudioUnitProperty_HostCallbacks` procs `notwig-rack-au` registers. The `ara`
feature forwards to `notwig-rack-au/ara`, so AU v3 instances expose the same
`notwig_rack_ara::AraPlugin` implementation.

## Usage

```toml
[target.'cfg(target_vendor = "apple")'.dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-au3 = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_au3::Au3Scanner;

let plugins = Au3Scanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option.
