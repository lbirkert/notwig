# notwig-rack-lv2

LV2 host implementation for the [**notwig-rack**](../README.md) audio plugin
framework, forked from [truce-rack](https://github.com/truce-audio/truce-rack).
Built on `lilv-sys` (host-side LV2 discovery / state library).

Implements scan, load, audio and MIDI atom-sequence processing, state, and the
LV2 UI extension (`CocoaUI` on macOS, `WindowsUI` on Windows, `X11UI` on Linux)
behind the format-agnostic `notwig-rack-core` traits.

Host transport from `ProcessContext::transport` is injected as a
`time:Position` object atom (speed, frame, BPM, beats-per-bar, beat unit, bar,
and bar-beat) at the head of the input atom sequence each block.

## System dependency

`lilv-sys` links against the system `lilv-0` library:

- macOS: `brew install lilv`
- Debian/Ubuntu: `apt install liblilv-dev`
- Fedora/RHEL: `dnf install lilv-devel`

Without it, `cargo build -p notwig-rack-lv2` fails at link time.

## Usage

```toml
[dependencies]
notwig-rack-core = { path = "../notwig-rack-core" }
notwig-rack-lv2 = { path = "." }
```

```rust
use notwig_rack_core::scanner::PluginScanner;
use notwig_rack_lv2::Lv2Scanner;

let plugins = Lv2Scanner::new().scan()?;
for info in &plugins {
    println!("{info}");
}
```

## License

MIT or Apache-2.0, at your option.
