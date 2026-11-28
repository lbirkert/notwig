# notwig-rack (umbrella)

Cross-format audio plugin host umbrella crate for the
[**notwig-rack**](../README.md) framework, forked from
[truce-rack](https://github.com/truce-audio/truce-rack).

CLAP, VST3, and VST 2.4 are enabled by default. LV2, Audio Unit v2, Audio Unit
v3, and the native editor host are opt-in; the Audio Unit formats are Apple-only
and LV2 needs the system `lilv-0` library. Every wrapper forwards host transport
from `ProcessContext::transport` into its backend, and the VST3, VST2, and AU
wrappers report plugin latency. The `editor` feature re-exports
`notwig-rack-editor` as `notwig_rack::editor_window`.

| Feature | Default | Crate it enables | Platforms |
| --- | --- | --- | --- |
| `clap` | yes | `notwig-rack-clap` | all |
| `vst3` | yes | `notwig-rack-vst3` | all |
| `vst2` | yes | `notwig-rack-vst2` | all |
| `lv2` | no | `notwig-rack-lv2` | all (needs `lilv-0`) |
| `au` | no | `notwig-rack-au` | Apple |
| `au3` | no | `notwig-rack-au3` | Apple |
| `editor` | no | `notwig-rack-editor` | all (native windows on macOS) |

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

To opt into LV2 and the Audio Unit formats as well:

```toml
[dependencies]
notwig-rack = { path = ".", features = ["lv2", "au", "au3"] }
```

## License

MIT or Apache-2.0, at your option.
