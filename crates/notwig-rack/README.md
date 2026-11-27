# notwig-rack

Cross-format audio plugin hosting for Notwig, forked from
[**truce-rack**](https://github.com/truce-audio/truce-rack).

This repository vendors the truce-rack host crates and extends them with
features that upstream does not support:

- **Plugin latency queries.** `PluginCore::latency_samples` is exposed by the
  VST3 (`getLatencySamples`), VST2 (`AEffect::initial_delay`), and AU
  (`kAudioUnitProperty_Latency`) wrappers so hosts can perform delay
  compensation.
- **ARA (Audio Random Access) hosting.** `notwig-rack-ara` provides the safe
  ARA 2.3 host runtime, in-memory audio/archive providers, and a checked
  document graph. CLAP, VST3, and AU v2 expose their companion discovery and
  role-aware binding entry points behind the `ara` feature.
- **VST 2.4 hosting.** The new `notwig-rack-vst2` crate hosts legacy `.vst`
  plugins using a clean-room VST 2 ABI, including scan/load, MIDI, state,
  latency, and editors.

The fork follows the same format-agnostic `Plugin` / `PluginScanner` /
`PluginEditor` traits as upstream. It is not affiliated with truce-audio.

## Crates

| Crate | Purpose |
| --- | --- |
| [`notwig-rack`](notwig-rack) | Umbrella crate; re-exports the wrappers behind features |
| [`notwig-rack-core`](notwig-rack-core) | Format-agnostic traits, buffers, events, transport |
| [`notwig-rack-clap`](notwig-rack-clap) | CLAP host |
| [`notwig-rack-vst3`](notwig-rack-vst3) | VST3 host |
| [`notwig-rack-vst2`](notwig-rack-vst2) | VST 2.4 host |
| [`notwig-rack-lv2`](notwig-rack-lv2) | LV2 host (needs system `lilv-0`) |
| [`notwig-rack-au`](notwig-rack-au) | Audio Unit v2 host (Apple platforms) |
| [`notwig-rack-au3`](notwig-rack-au3) | Audio Unit v3 host (Apple platforms) |
| [`notwig-rack-ara`](notwig-rack-ara) | ARA host runtime, document graph, and providers |

## Usage

```toml
[dependencies]
notwig-rack = { path = "notwig-rack" }   # CLAP + VST3 + VST2 by default
```

```rust
use notwig_rack::core::scanner::PluginScanner;
use notwig_rack::clap::ClapScanner;
use notwig_rack::vst3::Vst3Scanner;
use notwig_rack::vst2::Vst2Scanner;

let clap_plugins = ClapScanner::new().scan()?;
let vst3_plugins = Vst3Scanner::new().scan()?;
let vst2_plugins = Vst2Scanner::new().scan()?;
```

LV2 and Audio Unit are opt-in; Audio Unit is Apple-only:

```toml
[dependencies]
notwig-rack = { path = "notwig-rack", features = ["lv2", "au", "au3"] }
```

To keep the dependency graph minimal, depend on the per-format crates
directly instead of the umbrella.

## Feature summary

| Feature | Default | Crate | Platforms |
| --- | --- | --- | --- |
| `clap` | yes | `notwig-rack-clap` | all |
| `vst3` | yes | `notwig-rack-vst3` | all |
| `vst2` | yes | `notwig-rack-vst2` | all |
| `lv2` | no | `notwig-rack-lv2` | all (needs `lilv-0`) |
| `au` | no | `notwig-rack-au` | Apple |
| `au3` | no | `notwig-rack-au3` | Apple |
| `ara` | no | `notwig-rack-ara` + format `ara` features | all (AU only on Apple) |

## Differences from upstream truce-rack

- Package, library, and module names use the `notwig-rack` prefix.
- Latency reporting is part of the `PluginCore` trait and implemented by the
  VST3, VST2, and AU wrappers.
- VST2 is a first-class host format rather than "not planned".
- `notwig-rack-vst2` is new in this fork.
- ARA hosting is new in this fork; see [`notwig-rack-ara`](notwig-rack-ara).

## License

The host crates are MIT or Apache-2.0, at your option, matching upstream
truce-rack.

The VST 2 ABI translation in `notwig-rack-vst2/src/abi.rs` is BSD-3-Clause,
translated from Xaymar/vst2sdk. Parts of the VST2 event and time handling are
adapted from 115dkk/vst-audio; the DPF-derived layouts are ISC. See
[`NOTICE`](NOTICE), [`LICENSE-ISC-DPF`](LICENSE-ISC-DPF), and the license header
in `notwig-rack-vst2/src/abi.rs` for details.

VST is a registered trademark of Steinberg Media Technologies GmbH. This
project is independent of Steinberg and is not endorsed by it.
