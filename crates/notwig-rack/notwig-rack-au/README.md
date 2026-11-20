# notwig-rack-au

Audio Unit v2 host implementation for the [**notwig-rack**](../README.md)
audio plugin framework, forked from
[truce-rack](https://github.com/truce-audio/truce-rack). Apple-only and
pure Rust on top of the `objc2` ecosystem (`objc2-audio-toolbox`,
`objc2-core-audio-types`) — no C++ wrapper or bridging header.

Implements scan, load (both `AudioComponentInstanceNew` and async
`AudioComponentInstantiate` for AU v3-flagged components), audio and MIDI
processing, latency reporting (`kAudioUnitProperty_Latency`), and the AUv2
Cocoa view behind the format-agnostic `notwig-rack-core` traits.

Host transport from `ProcessContext::transport` is exposed through the AU host
callbacks (`kAudioUnitProperty_HostCallbacks`).

## Usage

```toml
[target.'cfg(target_vendor = "apple")'.dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-au = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_au::AuScanner;

let plugins = AuScanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option.
