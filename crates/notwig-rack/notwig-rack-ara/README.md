# notwig-rack-ara

ARA (Audio Random Access) host support for the
[**notwig-rack**](../README.md) audio plugin framework.

ARA is a companion extension to CLAP, VST3, and Audio Unit v2. This crate owns
the format-agnostic host side: host services, the checked document graph,
in-memory audio and archive providers, and the trait each format wrapper uses
to expose its native ARA binding entry point.

The safe ARA runtime comes from the [`ara2-bridge`](https://crates.io/crates/ara2-bridge)
0.3.0 crates. `ara2-bridge-host` is vendored under `../third_party/` with only
two additive accessors required to bind a companion processor and map audio
sources (`DocumentSession::controller_ref`, `DocumentSession::audio_source_id`,
and provider address accessors). See that directory's `FORK.md`.

## Usage

```toml
[dependencies]
notwig-rack = { path = "../notwig-rack", features = ["ara"] }
```

```rust
use notwig_rack::ara::{
    ApiGeneration, AraDocumentBuilder, AraFactory, AraPlugin, AudioSourceBuffer,
    AudioSourceProperties, MemoryArchive,
};
use std::sync::Arc;

# fn example(plugin: &mut impl AraPlugin) -> Result<(), notwig_rack::ara::AraError> {
let factory = plugin.ara_factory().expect("ARA plugin");
let mut document = AraDocumentBuilder::new(ApiGeneration::V23Final)
    .document_name("Song")
    .build(factory)?;
document.bind(plugin)?;

let source = document.create_audio_source(
    AudioSourceProperties::new(Some("Take"), "take-1", 48_000, 48_000.0, 2, false.into())?,
    Arc::new(AudioSourceBuffer::new(vec![vec![0.0; 48_000], vec![0.0; 48_000]])?),
)?;
let binding = document.session().audio_source_id(source)?;
let mut archive = MemoryArchive::new();
document.store_document_to_archive(&mut archive)?;
# Ok(())
# }
```

## Lifecycle and ordering

1. Load the plugin through its normal format wrapper.
2. Create the document from the plugin's `AraFactory`.
3. Bind the plugin **before** activation, state load, or editor creation.
4. Build the source/region graph through `AraDocument::session_mut`.
5. Keep the plugin loaded until the document is dropped.

The document initializes its factory exactly once, so use one document per
initialized factory per plugin instance.

## Providers

- `MemoryAudioAccess` is the default audio provider. It keys immutable planar
  `f32` buffers by the stable `AudioSourceId` returned by
  `AraDocument::audio_source_id`.
- `MemoryArchiving` is the default persistence provider. The document helpers
  register and unregister a borrowed `MemoryArchive` around each store/restore
  call; the plugin sees ordinary position-based archive I/O.
- Content, model-update, and playback providers are optional and passed through
  the document builder.

## License

MIT or Apache-2.0, at your option. The vendored `ara2-bridge-host` fork is
MIT OR Apache-2.0; ARA API headers and the upstream ARA SDK are Apache-2.0.
