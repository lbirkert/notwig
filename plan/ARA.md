# ARA support in notwig-rack

## Goal

Add framework-level ARA (Audio Random Access) hosting to the vendored
`notwig-rack` workspace for CLAP, VST3, and Audio Unit v2, without integrating
ARA into `notwig-app`, `notwig-engine`, or the UI. Capability detection should
reach `notwig-plugins` so a later app integration can use it.

## Research summary

- ARA is a companion extension to a plugin format, not a standalone format.
- `ARACLAP.h` is a small CLAP plug-in extension
  (`org.ara-audio.ara.pluginextension/2`); no SDK is required.
- `ARAVST3.h` adds `ARA::IPlugInEntryPoint` and role-aware
  `ARA::IPlugInEntryPoint2`. Both are tiny `FUnknown` extensions, so they can be
  declared in pure Rust with the existing `vst3` crate.
- `ARAAudioUnit.h` uses the private AU v2 properties `'AraF'` (factory) and
  `'AraE'` (role-aware binding), both guarded by the `'Ara!'` magic token.
- `ara2-bridge` 0.3.0 provides the safe ARA 2.3 host runtime
  (`ara2-bridge-host`) and raw ABI (`ara2-bridge-sys`). The host runtime builds
  without any C++ SDK.
- The published `ara2-bridge-host` hides the document-controller ref and the
  stable audio-source ID. Both are required to bind a companion and map source
  buffers, so the crate is vendored with additive accessors.

## Patch list

- `notwig-rack-core`
  - Add `PluginInfo::supports_ara`.
- `notwig-rack-ara` (new)
  - ARA document builder/lifecycle, factory/extension handles, `AraPlugin`,
    roles, default in-memory audio and archive providers.
- `notwig-rack-clap`
  - Scan ARA feature flags, cache the ARA extension, expose factory/binding.
- `notwig-rack-vst3`
  - Pure-Rust ARA entry-point COM interfaces, scan-time class matching,
    factory/binding, defer editor probing for ARA plugins.
- `notwig-rack-au`
  - Query `'AraF'` / bind through `'AraE'`, report capability on load.
- `notwig-rack-au3`
  - Forward the `ara` feature to `notwig-rack-au`.
- `notwig-rack`
  - New `ara` feature and re-export.
- `notwig-plugins`
  - Forward the `ara` feature; carry `supports_ara` on descriptors and
    capabilities.
- `third_party/ara2-bridge-host`
  - Pinned fork with `DocumentSession::controller_ref`,
    `DocumentSession::audio_source_id`, and provider address accessors.
- Docs
  - New `docs/03_development/25_ara_hosting.md`, plugin-hosting and crate
    reference updates, READMEs, NOTICE.

## Migration TODOs

- [x] Vendor/patch `ara2-bridge-host`.
- [x] Add `PluginInfo::supports_ara`.
- [x] Create `notwig-rack-ara`.
- [x] Add format/umbrella features.
- [x] CLAP discovery, factory, role-aware binding.
- [x] VST3 discovery, pure-Rust entry points, binding.
- [x] AU v2 discovery/binding; AU3 forwarding.
- [x] Tests with `ara2-bridge-testkit` and format ABI checks.
- [x] Framework docs and READMEs.
- [x] `cargo fmt`, workspace clippy, workspace tests, mdBook build.
- [x] Verify AU code compiles for `aarch64-apple-darwin` via the umbrella.
- [ ] App/engine integration: map audio clips to ARA sources and region
      sequences.
- [ ] UI integration: host ARA editor views and route editor parameter/model
      updates.
- [ ] Persist ARA archives in project bundles.
- [ ] Provide default content-access and playback providers bridged to Notwig
      transport/tempo.
- [ ] Runtime validation with real Melodyne/ARA test plugins on macOS.

## Integration notes

- One `AraDocument` initializes one factory; use a separate plugin instance for
  multiple documents.
- `AraPlugin::bind_ara` is unsafe because it consumes a raw controller ref; the
  document supplies only its own valid ref.
- Bind before activation, state load, and editor creation. VST3 loading skips
  its eager editor view probe for ARA processors for this reason.
- Plugin lifetime is a host contract, not a Rust borrow: keep the plugin loaded
  until the document is dropped.
- `MemoryAudioAccess` is `f32`-backed; `f64` reads are converted. Reads past the
  source end return zero.
