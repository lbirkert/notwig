# notwig-rack-clap

CLAP host implementation for the [**notwig-rack**](../README.md) audio plugin
framework, forked from [truce-rack](https://github.com/truce-audio/truce-rack).
Pure Rust on top of `clap-sys`.

Implements scan, load, audio and MIDI processing, state, parameters, and the
CLAP GUI extension behind the format-agnostic `notwig-rack-core` traits.
Host transport from `ProcessContext::transport` is passed through as a
`clap_event_transport`. Every plugin receives a real `clap_host` with the
standard host-extension surface, which plugins such as nih-plug require during
construction.

## Usage

```toml
[dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-clap = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_clap::ClapScanner;

let plugins = ClapScanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option.
