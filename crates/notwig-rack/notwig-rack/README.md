# notwig-rack (umbrella)

Cross-format audio plugin host umbrella crate for the
[**notwig-rack**](../README.md) framework, forked from
[truce-rack](https://github.com/truce-audio/truce-rack).

CLAP, VST3, and VST 2.4 are enabled by default. Audio Unit v2 is opt-in and
Apple-only. Every wrapper forwards host transport from
`ProcessContext::transport` into its backend, and the VST3 and AU wrappers
report plugin latency.

| Feature | Default | Crate it enables | Platforms |
| --- | --- | --- | --- |
| `clap` | yes | `notwig-rack-clap` | all |
| `vst3` | yes | `notwig-rack-vst3` | all |
| `vst2` | yes | `notwig-rack-vst2` | all |
| `au` | no | `notwig-rack-au` | Apple |

## Usage

```toml
[dependencies]
notwig-rack = { path = "." }
```

```rust
use notwig_rack::core::scanner::PluginScanner;
use notwig_rack::clap::ClapScanner;
use notwig_rack::vst2::Vst2Scanner;
use notwig_rack::vst3::Vst3Scanner;

let clap_plugins = ClapScanner::new().scan()?;
let vst3_plugins = Vst3Scanner::new().scan()?;
let vst2_plugins = Vst2Scanner::new().scan()?;
```

To opt into AU as well:

```toml
[dependencies]
notwig-rack = { path = ".", features = ["au"] }
```

## License

MIT or Apache-2.0, at your option.
