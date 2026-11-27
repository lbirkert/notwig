# ARA Hosting

`notwig-rack-ara` is the framework-level ARA (Audio Random Access) host layer
inside the vendored `notwig-rack` workspace. It is deliberately separate from
`notwig-app`, `notwig-engine`, and the UI: the framework can build and bind ARA
documents, but Notwig does not yet map arranger clips to ARA sources or host ARA
editors. Related: [`Plugin Hosting`](16_plugin_hosting.md) and
[`Crate Reference`](07_crate_reference.md).

## 1. Scope and layout

```text
notwig-rack-ara
  ├─ AraDocument / AraDocumentBuilder   host services + document graph + binding
  ├─ AraFactory / AraPlugin             format-agnostic discovery/binding handles
  ├─ AraRoles                           ARAPlugInInstanceRoleFlags wrapper
  ├─ MemoryAudioAccess                  default AudioAccessProvider
  └─ MemoryArchiving                    default ArchivingProvider
        │
        ├─ notwig-rack-clap   ARACLAP.h plug-in extension
        ├─ notwig-rack-vst3   pure-Rust ARAVST3 entry-point COM interfaces
        └─ notwig-rack-au     AU v2 'AraF' / 'AraE' private properties
```

The safe ARA runtime is the `ara2-bridge` 0.3.0 family (`ara2-bridge-core`,
`-sys`, and `-host`). `ara2-bridge-host` is vendored under
`crates/notwig-rack/third_party/ara2-bridge-host` because the published 0.3.0
API hides two values the companion binding and audio provider need:
`DocumentSession::controller_ref()` and `DocumentSession::audio_source_id()`.
`FORK.md` records the exact additive changes. Everything else is unchanged.

## 2. Feature flags

- `notwig-rack/ara` re-exports `notwig-rack-ara` and forwards `ara` to the CLAP,
  VST3, AU, and AU3 wrappers that are enabled.
- `notwig-rack-clap/ara` adds the CLAP ARA plug-in extension.
- `notwig-rack-vst3/ara` adds the VST3 ARA entry-point interfaces.
- `notwig-rack-au/ara` adds the AU v2 ARA properties; `notwig-rack-au3/ara`
  forwards to it.
- `notwig-plugins/ara` forwards the feature and carries `supports_ara` on
  descriptors and capabilities for future app-level integration.

`PluginInfo::supports_ara` is present regardless of feature state. CLAP reports
it from the descriptor feature list (`ara:supported` / `ara:required`). VST3
reports it during scan when an `ARA Main Factory Class` name matches an
`Audio Module Class` name, and refreshes it on load from the processor entry
point. AU reports it on load from the `'AraF'` property.

## 3. Companion discovery and binding

| Format | Discovery | Binding | Notes |
| --- | --- | --- | --- |
| CLAP | `org.ara-audio.ara.pluginextension/2` (plus `.draft/2`) | `bind_to_document_controller` | CLAP feature list also exposes capability during scan. |
| VST3 | `ARA::IPlugInEntryPoint2`, then `IPlugInEntryPoint` | `bindToDocumentControllerWithRoles`, legacy fallback | IIDs and vtables are declared in pure Rust; no C++ SDK. |
| AU v2 | `kAudioUnitProperty_ARAFactory` (`'AraF'`) | `kAudioUnitProperty_ARAPlugInExtensionBindingWithRoles` (`'AraE'`) | Both carry the `'Ara!'` magic token; roles are included in the binding record. |

VST3 loading defers its editor probe for ARA-capable processors, because the
specification requires binding before the companion creates a view. AU property
queries do not create a view.

## 4. Lifecycle

```text
load plugin (format wrapper)
  -> plugin.ara_factory()
  -> AraDocumentBuilder::build(factory)      // factory initialize + controller
  -> document.bind(&mut plugin)             // companion bind + extension attach
  -> session.edit() / create sources/regions
  -> plugin.prepare/activate/process        // ARA mode after binding
  -> document.store/restore archives
  -> drop document                           // leaf-first close, then factory uninit
  -> drop plugin
```

Ordering invariants:

1. Bind before activation, state load, `getProcessContextRequirements`, and
   editor creation.
2. The plugin that owns the factory must outlive the document.
3. One document initializes one factory once. Create separate plugin instances
   for multiple documents.
4. Document graph mutation uses the host runtime's edit scopes; never mutate it
   from the audio callback.

`AraDocument::bind` checks that the plugin's current factory pointer equals the
document factory before calling the companion, so accidentally binding a
different instance fails instead of producing a foreign graph.

## 5. Host services

`AraDocumentBuilder` installs the required audio-access and archiving providers
and optional content, model-update, and playback providers. The defaults are:

- **`MemoryAudioAccess`** maps `AudioSourceId` to immutable planar `f32`
  buffers. Readers support `f32` and `f64` requests, return zeros outside the
  source range, and are independent per plugin request. Buffers are keyed by
  the token from `AraDocument::audio_source_id(handle)`.
- **`MemoryArchiving`** reads and writes `MemoryArchive` buffers. The document
  helpers register a borrowed archive token around plugin store/restore calls;
  the provider never assumes the token's concrete type beyond the address key.

`AraDocument::create_audio_source` is the convenience path: it creates the
source, finishes the edit, registers the buffer, and enables sample access.
Custom audio/archive providers can be installed; then use the exposed
`DocumentSession` directly and register through the provider's own API.

## 6. Persistence

`MemoryArchive` carries bytes and an optional document archive ID. Complete
document storage uses `store_document_to_archive`; ARA 2 partial state uses
`store_objects_to_archive` with a `StoreFilter`. On restore, use
`restore_objects_from_archive` for ARA 2 Final and newer;
`restore_document_from_archive` is the pre-ARA-2 full-document path. The
archive ID must be compatible with the plugin factory, which the host runtime
checks before invoking the plugin.

## 7. Threading and real-time rules

- Factory initialization, document creation, binding, graph edits, and
  persistence run on the model/UI thread.
- Audio readers may run concurrently on plugin threads; `MemoryAudioAccess`
  uses a mutex only for the source lookup, not for sample copies.
- `AudioSourceBuffer` is immutable after construction; replacing audio requires
  a new buffer/source.
- Do not create or destroy document objects, allocate archive storage, or call
  plugin ARA callbacks from the real-time audio callback.

## 8. Testing

`notwig-rack-ara` uses `ara2-bridge-testkit` in dev-dependencies to build a
conformant mock factory without native plugins. Tests cover factory metadata,
audio source registration/reads (including reads past the end), archive
store/restore round-trips, and role constants. Format crates add ABI-level tests
for CLAP feature detection and VST3 IID presence. AU code is macOS-only and is
not exercised on Linux CI.

## 9. Current gaps

- No Notwig app/engine/UI integration: arranger clips are not mapped to ARA
  audio sources, ARA editors are not opened, and ARA archives are not persisted
  in project bundles.
- No default content-access provider (tempo/notes/chords) or playback-control
  bridge to the Notwig transport.
- `MemoryAudioAccess` stores `f32`; `f64` reads are converted, not native.
- AU ARA discovery/binding is implemented against the published property layout
  but cannot be compiled or exercised in this Linux environment.
- Factory lifetime is a host contract, not a Rust borrow: dropping a plugin
  before its document is undefined.
