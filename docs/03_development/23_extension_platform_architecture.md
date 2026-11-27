# Extension platform architecture

Status: proposed platform design, with the first migration (Phase 1 commands)
implemented. This page evaluates an Eclipse-style extension-point platform for
Notwig. The current device runtime contract is in
[`Extension ABI and Hosts`](19_extension_abi_and_hosts.md), the panel transport
is in [`Extension Panels`](12_extension_panels.md), and the existing extension
plan is in [`plan/EXTENSIONS.md`](../../plan/EXTENSIONS.md).

Related: [`Architecture`](00_architecture.md),
[`Code Structure`](01_code_structure.md),
[`Command Application and Undo`](13_command_application_and_undo.md),
[`UI Contracts and Coordinates`](17_ui_contracts_and_coordinates.md),
[`Application Lifecycle`](18_app_lifecycle.md), and
[`plan/MOVING_DEVICES.md`](../../plan/MOVING_DEVICES.md).

## 1. Short answer

A full "every subsystem is an independently replaceable extension" model is not
a realistic first step. It would put canonical project data, undo, audio
execution, focus, and the shell behind contracts at the same time, before the
contracts themselves have users. The bootstrap cost and the compatibility
surface would be larger than the features they enable.

A narrower platform is feasible and useful: one deterministic contribution
registry, typed host-owned extension points, namespaced extension-defined
points, explicit contribution modes, and a small number of replaceable service
slots with a built-in fallback. Built-ins can publish through the same registry
while remaining compiled-in and guaranteed. That gives third parties a real
extension route without making the core optional.

| Question | Verdict |
| --- | --- |
| Eclipse-style extension points | Feasible as a contribution registry plus contracts. Not feasible as blanket replacement of every subsystem in one migration. |
| Recursive extension points | Feasible for extension-owned, namespaced points with opaque payloads. The host must remain the authority for capabilities and host UI/engine integration. |
| Built-ins as ordinary extensions | Partially. Core providers can use the same manifest/contribution model while compiled-in, trusted, and non-removable. Creative devices and formats can be bundled packs. Project/undo/engine/shell should not become optional. |
| First migration | Commands with a palette, dynamic bindings, and a bounded context; audio-editor sub-editors; and shell-hosted extension panels. Clipboard formats follow after these surfaces are proven. |
| Global replaceable systems such as clipboard | Feasible through a service slot with one active provider and a built-in fallback, but the useful step is extensible clipboard formats and paste handlers, not replacing the clipboard owner. |
| Load-order-independent packaging | Feasible with a pure resolver, explicit ordering keys, one active version per extension ID, deterministic tie-breaks, atomic activation, and a lockfile for packs. |
| Compile-time performance of built-ins | No audio-path cost if resolution happens at plan/build time. Costs are binary size, compile time, and startup registration, not per-sample dispatch. |
| Maintainability | Manageable with few contracts, contract owners, conformance tests, and diagnostics. A broad platform without those controls is hard to maintain. |
| API looseness | Loose envelopes and opaque extension-to-extension payloads are viable. Host services and project mutation must stay typed and capability-gated. |
| Migration risk | High for a big-bang rewrite. Medium and controllable for an additive, phased migration that keeps the existing device path working. |

The recommended end state is "an extensible platform with a guaranteed core",
not "replace everything". The distinction is deliberate: a platform needs a
stable floor to boot, recover, render, and explain failures when extensions are
missing or broken.

The concrete first migration is defined in section 8: dynamic commands and a
command palette, audio-editor sub-editors, and extension panels. Those three
surfaces are the acceptance test for the registry, context, command, and panel
contracts. Phase 1 commands and extension-rendered audio sub-editors are
implemented; general extension panels and clipboard formats remain follow-ups. The author-facing contracts are in
[`Command API`](../02_extensions/06_command_api.md) and
[`Audio Sub-editor API`](../02_extensions/07_audio_sub_editor_api.md).

## 2. Current baseline

The existing extension subsystem is device-specific but already contains most
of the hard runtime pieces:

- A package directory with `extension.json` and ordered target entrypoints.
- Discovery that parses and validates manifests without executing extension
  code (`crates/notwig-ext/notwig-ext/src/discovery.rs`).
- Exact runtime ABI version `6`, with generated WASM and native exports around
  the `DeviceExtension` trait.
- WASM, native shared-library, and browser transports behind one host API.
- Device specs merge into the same `DeviceRegistry` used by stock devices, and
  the app composes `StockDeviceFactory`, the extension factory, and the plugin
  factory.
- Guest panels run a private `egui::Context` and return validated meshes,
  texture deltas, and parameter actions. Extensions never receive the host
  context, project document, or a host pointer.
- Project mutation is not exposed. UI emits `UiIntent`, the app lowers it into
  `CommandPayload`, `notwig_core::apply` validates and produces a new document,
  and `Workspace` records history. Extensions cannot bypass that path.

The current extension manifest has only `devices`, `capabilities`, and target
metadata. It has no extension points, contributions, dependencies, or service
bindings. Unknown manifest fields are rejected, so adding this model requires a
new manifest schema version, not a silent reinterpretation of v5 packages.

Several "shared" systems are currently app/UI internals rather than services:

- The clipboard is a `notwig-ui` enum with `Arranger`, `AudioClip`, `Devices`,
  and `Empty` variants (`crates/notwig-ui/src/clipboard.rs`).
- Device copy data lives separately in the workspace device clipboard, and the
  MIDI editor keeps its own note clipboard.
- The OS clipboard is only used as a marker (`copy_text("notwig-clipboard")`);
  Notwig data is not round-tripped through the platform clipboard.
- Cross-domain paste is rejected rather than converted.
- Undo history stores whole `ProjectDoc` snapshots and is owned by the app.
- The engine owns playback and receives prepared runtimes; the audio callback
  never resolves factories or loads plugins.

These are the boundaries the platform design must preserve.

## 3. What an extension point is

An extension point is a named, versioned contract plus a resolution policy. It
is not a Rust trait exposed across the package boundary, and it is not a free
function pointer. It has five separate identities:

| Identity | Example | Owner |
| --- | --- | --- |
| Extension ID | `com.example.clip-tools` | Package author |
| Point ID | `notwig.clipboard.format` | Point owner |
| Contract ID and version | `notwig.clipboard.format@1.0` | Point owner |
| Contribution ID | `com.example.clip-tools.arranger-json` | Contributor |
| Binding ID | resolved instance in an activation generation | Host resolver |

The contract declares:

- the contribution modes it accepts (`replace`, `augment`, `middleware`,
  `observe`);
- cardinality for each mode (one, many, or exactly one active replacement);
- the input and output payload schemas;
- the lifecycle (pure function, long-lived service, per-instance object);
- the thread and real-time class (UI, worker, audio block, audio sample);
- the capabilities the implementation requires;
- the compatibility policy for contract versions.

A contract version is independent of the package version, the runtime ABI, and
the manifest schema. A package may update without changing a contract, and a
contract may add an optional capability without invalidating every package. The
current device ABI keeps its exact-match policy; a general platform cannot use
one global exact version for everything without constant rebuild churn.

### Contract ownership and namespaces

Point IDs use reverse-DNS namespaces:

- `notwig.*` points are declared and owned by the host or built-in providers.
  External packages cannot declare them.
- `<extension-id>.*` points are owned by the declaring extension. Other
  extensions may contribute to them if the contract accepts contributions.
- IDs are case-sensitive, validated as stable identifiers, and never derived
  from a filesystem path or display name.

An extension that declares a point publishes its contract metadata in the
manifest or in a package-local contract file. Discovery reads that metadata but
does not execute the extension. The registry can therefore list points even
when the implementation cannot be loaded.

### Contributions are not authority

A contribution grants only the capability of its declared point. It does not
grant arbitrary access to the project, UI, audio callback, filesystem, network,
or OS clipboard. A clipboard format handler can read a clipboard snapshot and
return a validated edit plan; it cannot call `apply` directly. A device
contribution can process an audio block; it cannot start a thread or mutate the
document.

This is the most important correction to a literal Eclipse model. Eclipse
extensions run in one JVM with broad classpath authority. Notwig extensions have
different trust levels, including sandboxed WASM, trusted native code, and
in-process built-ins. The registry must model authority explicitly.

## 4. Registry, resolver, and activation

The platform needs four distinct phases. Mixing them is the usual source of
load-order bugs.

```text
discover            read installed packages and metadata only
  -> validate       schema, IDs, versions, paths, capabilities, size limits
  -> resolve        pure function over the candidate set and host policy
  -> activate       instantiate providers and publish one registry generation
  -> retire         deactivate the old generation after the new one is ready
```

### Discovery

Discovery scans configured roots, reads manifests, validates paths, and
computes content hashes. It must not load native libraries, instantiate WASM,
run constructors, or call extension code. The current device discovery already
follows this rule; the general registry must keep it.

Installed packages are immutable by version. An install writes a new package
directory, validates it, then atomically changes the active pointer or lockfile.
A pack is not a special runtime unit; it is a distribution unit containing
several independently identified extensions.

### Resolution

Resolution is a pure function of explicit inputs:

- the set of installed packages and their manifests;
- hashes and versions from the lockfile or install record;
- user enable/disable choices and trust settings;
- host policy (which points are open to external contributions);
- the dependency constraints of every enabled extension.

It never depends on directory iteration order, activation order, the current UI
selection, or which extension happened to initialize first. The output is an
immutable `ResolvedRegistry`:

- active point declarations;
- active contributions per point and mode;
- concrete ordering for `augment` and `middleware` chains;
- the selected replacement, if any;
- unresolved packages and the reason;
- a generation number and a deterministic hash of the plan.

If resolution cannot produce a deterministic, valid result, it fails the whole
generation or disables the conflicting package. It never silently picks the
first package on disk.

### Activation

Activation is transactional. The host builds all providers for the new
generation, then publishes them together. If any required provider fails to
activate, the host keeps the previous generation and reports a typed activation
error. A partial generation must not be visible to the UI, engine build, or
extension consumers.

Long-lived services are activated off the UI thread where possible. The current
device host loads on the UI thread at startup; general activation should improve
on that, and module compilation must never run in a frame or audio callback. The
audio block path must never call into the resolver or activation runtime.

### Retiring

The old generation is retired only after in-flight work no longer references
it. Device runtime plans already snapshot what they need, so an old provider is
not consulted during playback. The platform should use the same rule for
services: a paste operation or editor tool holds a resolved generation for its
duration, then releases it.

## 5. Contribution modes

The four proposed modes are useful only if their semantics differ. Treating
them as synonyms for "some extension code runs" produces an unpredictable
system.

| Mode | Meaning | Cardinality | Ordering | Failure behavior |
| --- | --- | --- | --- | --- |
| `replace` | Provides the whole implementation of a single-provider point. | Exactly one active provider, plus host fallback. | Not ordered; host selection is explicit. | Replacement fails activation: keep fallback and report. Duplicate replacements: unresolved conflict. |
| `augment` | Adds a new independently selectable item (format, operation, analysis pass, export sink). | Many. | Stable ordering for listing and deterministic selection; add-ons do not override each other implicitly. | A broken contribution is marked unavailable; other contributions stay active. |
| `middleware` | Intercepts or transforms an operation at a declared boundary. | Ordered chain. | Explicit `before`/`after` edges plus stable tie-break, or a host-defined phase order. Cycles are rejected. | A failed middleware is skipped or fails the operation according to its contract; it cannot corrupt the generation. |
| `observe` | Receives committed events or snapshots. | Many. | Deterministic event order, but observers cannot change the result. | Observer failure is isolated and logged; it cannot block the operation. |

### Replace

`replace` is for a service whose interface is fully defined and whose entire
implementation is interchangeable. It is not a way to override one function in
a larger subsystem.

The host must still enforce:

- the replacement satisfies the full contract;
- the replacement cannot change canonical ownership (for example, a clipboard
  replacement still writes through the host clipboard and still asks the host
  to execute edit plans);
- the fallback remains available if activation fails;
- exactly one replacement is selected, and the selection is explicit policy or
  a pinned user setting, never "last one loaded".

Typical candidates later: a theme/palette provider, an export encoder for one
format, or a single-provider analysis backend. Poor first candidates: undo,
project persistence, the audio engine, the command model, and the shell.

### Augment

`augment` is the default extension mode. It adds items to a collection:

- a clipboard format handler;
- an audio-editor operation;
- an analysis pass;
- an export destination or encoder;
- a device type;
- a UI inspector section with a host-defined layout contract.

Each item has its own stable ID and can be enabled or disabled independently.
Aggregation is deterministic but order-insensitive unless the contract declares
that listing order is user-visible. Even then, the order comes from explicit
keys such as a category and display name, not discovery order.

### Middleware

Middleware is the mode most likely to cause surprises and real-time
regressions. It must be restricted by the contract:

- The contract names the exact phase where interception occurs. "Before
  everything" is not a phase.
- The chain is resolved once and stored in the plan for hot paths.
- Audio middleware runs at block boundaries or in the plan compiler, never per
  sample. Anything per-sample belongs inside a device processor, not in a
  generic middleware chain.
- The chain exposes only the payload of that phase, not arbitrary host state.
- A middleware declares whether it can short-circuit, mutate, or only observe.
- Cycles and duplicate chain positions are rejected at resolution.

For example, a MIDI-note-operation chain could let a quantize middleware
transform a note plan before the host validates and applies it. That is safe
because it runs on the user-action path. A per-sample "audio gain middleware"
would be a bad contract; make it a device instead.

### Observe

Observers receive committed facts: "clipboard changed", "project saved",
"device activated", "undo entry recorded". They cannot veto, reorder, or mutate
the event. This prevents cross-extension coupling from becoming hidden control
flow.

Observers still need deterministic delivery: the resolved order is stable, and
events are delivered after the state change is committed. An observer that
needs to request a follow-up change emits an intent through the host, which
validates and applies it normally.

## 6. Load order and packaging

The requirement "the order in which extensions load must not change behavior"
is a property of the whole system, not just the installer. It needs five
guarantees.

### 6.1 One active version per extension ID

The installed set contains at most one active version of an extension ID.
Upgrades replace the active pointer atomically. A pack that contains the same
ID twice is invalid. This removes "which copy won" from every later question.

### 6.2 Stable dependency resolution

`requires` constraints use a declared range syntax, and resolution sorts
candidates deterministically. In an installed desktop app the lockfile normally
pins exact versions, so resolution is mostly validation. A repository/index
resolver may choose the highest compatible version, but it must use a total,
documented ordering and record the choice in the lockfile.

Optional dependencies are explicit. A missing optional dependency disables only
the contributions that declared the dependency, not the whole generation.

### 6.3 Declared ordering, not directory ordering

Every ordered mode has an explicit order source:

- service replacement: host policy or user selection, at most one;
- middleware: `before`/`after` edges inside a named phase, then a stable
  tie-break;
- augment items: category/name/ID for display, or a contract-specific key;
- observers: resolved order for delivery, with no result dependency.

The stable tie-break is `(point_id, extension_id, contribution_id)`. It is a
last resort for reproducibility, not a way for authors to control behavior by
choosing an ID. If two contributions genuinely conflict, resolution fails with a
diagnostic instead of choosing one by bookkeeping order.

### 6.4 Atomic pack installation and activation

A "pack" is a signed or hashed distribution set. Installing a pack performs:

1. validate every package and the lockfile;
2. stage all package directories in a content-addressed store;
3. resolve the whole set;
4. run activation probes for required providers;
5. atomically switch the active generation.

If any step fails, the previous generation remains active. Installing a pack is
not the same as merging all its extensions into one privileged module. Each
extension keeps its own ID, version, capabilities, and contribution set.

### 6.5 No ambient initialization

An extension must not initialize global host state when its binary is loaded.
All state belongs to a provider instance created during activation. A module
constructor that mutates a process-global registry reintroduces load-order
dependence outside the resolver.

For native shared libraries this is a real risk because library constructors run
at load time. The contract should prohibit side effects in constructors, and the
host should load native libraries only after resolution, not during discovery.

### Resolution sketch

```text
given installed packages P, lockfile L, settings S, policy H:
  enabled = filter(P, S.enabled, S.trust, H.allowed_capabilities)
  validate_manifests(enabled)
  check_unique_ids_and_versions(enabled, L)
  graph = build_requirement_graph(enabled, L)
  reject_cycles(graph)
  points = collect_point_declarations(enabled, H)
  contributions = validate_and_group(enabled, points)
  for each point:
      selects = deterministic_sort(contributions[point], point.mode)
      reject_duplicate_replace(selects.replace)
      reject_middleware_cycles(selects.middleware)
      choose_replacement(policy, user_selection, host_fallback)
  plan = ResolvedRegistry { points, contributions, order, generation_hash }
```

The resolver has no clock, no filesystem side effects, and no calls into
extension code. That property makes it testable by shuffling input order and
comparing the resolved hash.

## 7. Recursive and extension-defined points

Recursive extensibility is feasible if extension-defined points are treated as
namespaced contracts with limited authority.

- An extension declares a point in its manifest: point ID, contract ID and
  version, accepted modes, payload schema or opaque-payload declaration,
  cardinality, and lifecycle.
- Contributions to that point are resolved by the same registry rules.
- The host does not interpret an extension-defined payload. It validates the
  envelope, size, origin, and declared version, then hands the payload to the
  consumers that declared the contract.
- A point owner that is disabled or removed disables the point and marks its
  dependent contributions unresolved. Consumers must handle "point missing"
  without panicking.
- Extension-defined points cannot grant new host authority. They can share data
  and call host services that the host already exposes.

This supports ecosystems such as "a notation extension defines an articulation
point; other extensions contribute articulation renderers or importers" without
changing Notwig. It does not support "a new extension point automatically
appears in the host Arranger with full interaction". That still requires host
code to consume the point.

Contract versions for extension-defined points should follow the same rule as
host points: a contribution targets a specific contract major and declares the
minor it was built against. A consumer can accept older minor payloads. The host
can only enforce this if the contract metadata is machine-readable; an opaque
payload with no version is still allowed, but the registry cannot protect the
consumer from semantic mismatch.

### Cross-point dependencies

A cycle in extension `requires` is always invalid. A cycle between contribution
graphs is allowed only when it is an observation relationship. If activation of
A needs data produced by B's contribution and B needs data from A, resolution
must reject it unless the contract explicitly defines a two-phase or lazy
resolution order. "It happens to work because A was loaded first" is not an
acceptable resolution.

## 8. First migration use cases: commands, audio sub-editors, and panels

The first migration should prove the platform against three concrete surfaces
rather than starting with the broadest replacement story:

1. extension commands with a VS Code-style palette, dynamic keybindings, and a
   bounded command context;
2. audio-editor sub-editors, both shared-curve editors and custom editors;
3. extension panels hosted by the shell.

These three share one pattern: the host owns the shell, the canonical state, the
undo boundary, and the layout; extensions supply small content or operation
providers and receive bounded snapshots. This is a better first proof than
replacing the clipboard service, because it exercises commands, UI surfaces,
editor state, and command application together without making canonical state
replaceable.

### 8.1 Dynamic commands, context, palette, and bindings

Today commands are a closed `ShortcutCommand` enum, `COMMANDS` is a static
table, `Shortcuts` is keyed by that enum, and `handle_shortcuts` contains
hardcoded matches (`crates/notwig-ui/src/commands/`). A command palette, a
command added by an extension, or a command whose enablement depends on the
focused editor cannot be represented without changing those types.

The first migration should introduce a `CommandId` string and a resolved command
registry while keeping the enum-backed built-in implementations during the
transition.

#### Command contribution

`notwig.command` is an augment point. One contribution declares:

```json
{
  "id": "com.example.midi-tools.humanize",
  "point": "notwig.command",
  "contract": "notwig.command@1",
  "mode": "augment",
  "title": "Humanize Notes",
  "category": "MIDI",
  "description": "Randomize the timing and velocity of selected notes.",
  "when": "focus == midi_editor && selection.notes > 0",
  "default_bindings": ["ctrl+alt+h"],
  "handler": "humanize_notes"
}
```

`when` is a small declarative expression over stable `CommandContext` fields,
not executable extension code. The host parses and validates it during
resolution. This keeps palette filtering and shortcut enablement cheap and
prevents a broken handler from making the palette unusable. A command that needs
more context checks it again in the handler and returns a typed error.

#### Command context

The host builds an immutable, bounded snapshot when the command runs:

```text
CommandContext
  revision: u64
  focus: timeline | audio_editor | midi_editor | device_editor | panel
  selection:
    tracks, clips, devices: bounded ID lists
    time: optional range and lanes
    notes: selected notes for the active MIDI clip, bounded
    slices: selected slices for the active audio clip, bounded
  active: optional track, clip, device, editor id, sub-editor id
  transport: playhead, play start, playing, tempo, time signature
  audio: sample rate
  ui: snap setting and active panel ids
  clipboard: available format IDs only
  capabilities: contribution grant and trust level
```

The selected-object lists are capped, like panel frames and clipboard payloads.
If a selection exceeds the cap, the command is disabled with a reason instead
of receiving a truncated or inconsistent view. A command that needs a larger or
different query uses a host query service with the same revision; it does not
receive the whole `ProjectDoc`.

`revision` is the document revision from the command's snapshot. Before applying
any effect, the host checks it against the current document. A stale context
fails with a "command context is stale" error instead of applying an edit to a
selection that has moved or been deleted.

#### Command effect

A command does not receive a mutable document and cannot call `apply`. It returns
a `CommandEffect`:

```text
CommandEffect
  None
  Edit(CommandEdit)       // host-owned operation families
  Ui(CommandUiRequest)    // select, focus, open panel/editor, notify
  Error(message)
```

The first migration supports a small set of edit families that map to existing
commands:

| Edit family | Maps to |
| --- | --- |
| `EditMidiNotes` | `SetNotes` / lowered note edits |
| `EditAudioSlices` | `SetClipSlices` / lowered slice edits |
| `EditClipFields` | `EditClip`, `SetClipColor`, `SetClipRange` |
| `SetTrackMixer` | `SetTrackMixer` |
| `SetDeviceParam` | `SetParam` for a validated device/parameter |

The host lowers the effect, allocates any new object IDs, validates the resulting
document, applies one batch, and records one undo entry. A new edit family that
touches canonical state still requires host code. Extension commands can compose
existing families freely, and extension-defined command result types can be
consumed by extension-defined handlers, but they cannot become canonical edits
without a host operation contract.

`CommandUiRequest` covers selection updates, focus changes, opening a panel or
editor, and notifications. The host validates IDs and focus transitions. A
command does not get to mutate arbitrary UI state.

#### Humanize example

A MIDI humanize command:

1. receives the active MIDI clip, selected note IDs, and their note values;
2. computes perturbed starts, lengths, and velocities with a deterministic seed
   supplied by the host context (or an explicit user seed);
3. returns `EditMidiNotes` with the changed notes and unchanged IDs;
4. the app lowers it to the existing note command path, applies one batch, and
   keeps the note selection;
5. the MIDI editor re-renders from the new document revision.

This demonstrates context reading, selection-sensitive enablement, editor
update, undo, and command palette integration without giving the extension
document authority.

#### Palette and keybindings

The palette is a host panel:

- It lists built-in and extension commands in the resolved generation.
- It filters by title, category, ID, and a short description.
- It hides commands whose `when` expression is false by default; an "all
  commands" view can show them with the reason (for example, no MIDI editor).
- It shows the effective keybinding, if any.
- It can open a command's contributed panel or focus the relevant editor after
  the effect succeeds.

`Shortcuts` must move from `BTreeMap<ShortcutCommand, _>` to
`BTreeMap<CommandId, _>`, with built-in enum variants mapped to stable string
IDs. The existing persisted keys are already strings such as `copy` and
`new_audio_track`, so the config format can remain compatible. Default bindings
come from the resolved command descriptors, not from the static enum table.

Keybinding conflict policy must be deterministic:

1. an explicit user binding wins over a contributed default;
2. if two enabled commands still share a chord, the chord is ambiguous and the
   host reports it in the shortcut editor;
3. context-specific bindings can coexist when their `when` expressions are
   mutually exclusive;
4. built-in default bindings are not silently overridden by extension defaults.

The last rule matters for predictability. An extension can suggest a binding,
but it cannot steal `space`, `delete`, or `command+z` from a built-in command.
The user can assign the conflict explicitly.

#### Execution and latency

Command handlers run on the UI/app thread for the first migration. They receive
a bounded context and must return within a short budget. WASM commands remain
subject to fuel and memory limits; native commands are trusted. A command that
needs long-running work returns a request for a host analysis/import job or a
typed "not now" status, then delivers an event when the result is ready. No
command handler runs on the audio thread.

### 8.2 Audio editor sub-editors

The host-owned selector is implemented. The audio editor strip header is now a
sub-editor menu with built-in `Audio Events` and `Gain` entries
(`crates/notwig-ui/src/views/audio_editor/`). Audio Events keeps the slice lane;
Gain uses the reusable `widgets::line_editor` over the clip's canonical
`gain_curve`. The line editor supports point insertion and movement, segment
curvature, segment value moves, marquee selection, grid snapping, and fine
adjustment. Switching surfaces clears in-progress drags and preserves the clip
time mapping, playhead, loop range, snap grid, and scrollbar. Extensions add
extension-rendered sub-editors through `notwig.audio_editor.sub_editor`. The
extension registers its editor under the contribution id; the example package
contributes Clip Pan that draws the shared curve editor over a waveform underlay
and commits a host-owned pan curve edit.

`notwig.audio_editor.sub_editor` is an augment point. A contribution declares:

```text
AudioSubEditorSpec
  id, title, icon (host-defined icon id), category, order
  renderer = contribution id, registered by the extension
  accepted_clip_kinds: audio
  singleton_per_clip: bool
```

The host owns:

- the tab button/menu that replaces the `"AUDIO EVENTS"` label;
- the active sub-editor choice in `ClipEditorState` (ephemeral UI state);
- the clip time mapping, playhead, loop range, snap grid, and scrollbar;
- the surrounding panel, focus, keyboard suppression, and close behavior;
- applying edits through commands and one undo group.

`Audio Events` becomes the first built-in contribution, not a hardcoded special
case. The menu lists it first, then extension sub-editors by category, title, and
ID. Disabling an extension removes its entry but does not silently switch the
active editor to a different one; the host falls back to Audio Events with a
notice.

#### Reusable curve editor

Gain is a host built-in over the clip's canonical `gain_curve`. Extensions can
use the same curve editor through the `notwig-ui` `extension-ui` feature:

- points, interpolation, add/remove, drag, keyboard nudge, and snapping;
- a value range, baseline, and formatter chosen by the extension;
- a read/write binding through host-owned edit families;
- a preview and commit path that emits one command batch;
- selection and viewport shared with the audio editor.

The host also supplies `AudioWaveformPreview` columns so an extension can draw
the same waveform underlay as the built-in editor. The extension still cannot
store playback-affecting curve state privately; edits return as `CommandEffect`
and the host validates the revision before applying them.

This exposes a real model gap that the design must not hide:

- `AudioClipData` and `AudioSlice` already have `gain_db`, so a first Gain
  sub-editor can edit existing clip/slice gain without a new document field. A
  time-varying gain envelope still needs a host model and engine path.
- There is no pan field on audio clips or slices today. A true Pan envelope
  requires new canonical data, command support, engine rendering, and
  persistence before an extension can edit it.
- A curve that only lives inside the extension cannot affect playback and is not
  a canonical edit. It must either be host-modeled, produce a host command, or
  be rendered into an asset through a host job.

The correct sequence is to build the shared curve widget and the host data
binding first, then add the canonical fields. The extension point does not grant
extensions a private place to store playback-affecting data.

#### Custom sub-editors

A pitch-correction editor is a custom sub-editor: it needs its own interaction
model, analysis, and likely a different visualization. It should use the panel
surface protocol described in section 8.3, scoped to the audio-editor content
area.

The custom path is implemented. The host supplies an `AudioSubEditorContext`:

```text
clip id and range; active asset metadata; sample rate
selected slice IDs and values; current sub-editor and panel size
playhead, loop, tempo, snap; project revision
available analysis results or analysis request handle
theme and input snapshot
```

The extension draws with the panel surface and returns `CommandEffect` built
from the same validated edit families. It does not receive the full document or
a host pointer. The host checks the captured revision before applying edits and
uses one undo batch per effect.

For pitch correction specifically, separate the UI from the processing:

- **UI-only prototype:** demonstrate the custom sub-editor slot and its
  selection/analysis/command plumbing without claiming pitch correction data
  affects playback.
- **Host data contract:** if correction points must persist and affect playback,
  add a host-owned, namespaced extension-data area attached to the clip (or an
  equivalent canonical field) with versioning, bounds, commands, and an engine
  read path.
- **Device/processor route:** contribute a device or processor that consumes the
  host-modeled data. This is the preferred route when the processing is already
  device-shaped.
- **Destructive route:** render a corrected asset or slices through a host job
  and replace the clip content through normal commands. This is valid for an
  offline workflow but must be explicit and undoable.

The first migration should pick one route for the pitch-correction proof. Do not
ship a UI that writes hidden extension state which the engine silently ignores.

Sub-editor state is keyed by `(clip_id, sub_editor_id)` in the UI, so switching
away and back preserves the view. The document data is always read from the
current revision; a stale context fails before committing an edit.

### 8.3 Extension panels

The device panel path already proves the hard part: a guest runs a private egui
context, returns a validated `PanelFrame`, and the host composites meshes and
textures into a bounded rectangle. General extension panels should reuse that
transport instead of inventing a second UI protocol.

`notwig.panel` is an augment point. A contribution declares:

```text
PanelSpec
  id, title, icon, category, order
  location: left | right | bottom | floating
  default_size, min_size
  singleton: true (first migration)
  kind:
    GuestPanel     // extension draws, host owns chrome
    HostPanel      // extension declares a small control schema
  accepts_context: global | selection | device | audio_editor
```

The host owns the dock/tab/window chrome, placement, resize, close, focus order,
and keyboard routing. A guest panel draws only inside its surface. A "floating"
location means a host-managed floating panel, not a guest-created OS window.
This is the same limitation as device panels and should be stated in the author
docs.

A generalized `PanelContext` contains the bounded context from section 8.1 plus
panel identity, size, `pixels_per_point`, input snapshot, theme, and repaint
hint. The host validates the returned frame with the existing panel limits and
texture rules. The device-specific `DevicePanelHost` and `DeviceDrawContext`
remain for device-panel compatibility; a new host API can share the frame codec and
compositing implementation without changing the device wire format.

Panel actions should use the command effect surface, not a second unchecked
action enum:

- apply a validated edit batch;
- select or focus a host object;
- open another panel or editor;
- request analysis or a repaint;
- show a notification.

Device parameter actions remain the existing `PanelAction` path. A general panel
that wants to edit a device parameter can use the `SetDeviceParam` family.

Panel lifecycle:

- one guest instance per visible panel (or per singleton panel ID);
- retained state across frames and reopen;
- close/destroy calls the extension lifecycle hook;
- a panel cannot create host widgets outside its surface;
- keyboard and text events are forwarded only while the panel owns focus;
- host shortcuts are suppressed while the panel wants keyboard input;
- an invalid frame falls back to the last valid frame or a host placeholder.

Open/closed state and placement are user UI state, not project document state.
A panel that needs durable data uses a host storage capability; canonical
project data is never stored in extension UI state.

Panels and commands compose directly: a command's `CommandUiRequest` can open a
contributed panel, and a panel button can return a command effect. This gives the
first migration a complete loop without arbitrary host UI.

### 8.4 What the first migration should not promise

- Arbitrary extension-created command payloads that mutate canonical state
  without a host operation family.
- True Gain or Pan envelopes before the canonical fields, engine path, and
  persistence exist.
- Pitch correction that affects playback through hidden extension state.
- Free-floating guest windows outside the host panel surface.
- Replacing the clipboard service. Clipboard format handlers remain a later
  milestone; the clipboard contract in section 9 is the target, not the first
  deliverable.
- General overlays on the arranger, clip lanes, or host focus order.

These are deliberate cuts. Each one can be added after the command, panel, and
sub-editor plumbing is real and tested.

## 9. Global replaceable systems: the clipboard case study

The clipboard is the best stress test for the proposal because it is global,
cross-editor, stateful from the user's perspective, and currently split across
several owners.

### 9.1 Current state and problems

- `notwig_ui::Clipboard` is a closed enum of arranger, audio, and device kinds.
- The arranger extract and audio extract are stored in the shared workspace
  clipboard; device snapshots live in a separate `DeviceClipboard`.
- The MIDI editor has a private `NoteClipboard`.
- Paste rejections are hardcoded ("Clipboard belongs to another editor").
- The OS clipboard only receives a marker string, so copying between Notwig
  instances or from external tools is not supported.
- Adding a new editor or a third-party clip type would require modifying the UI
  enum, the workspace, the shortcuts, and every paste match.

### 9.2 Proposed clipboard contract

Keep the host as the clipboard owner. Extend what can be copied, not who owns
the canonical clipboard.

A clipboard snapshot is a versioned envelope:

```text
ClipboardSnapshot
  generation: u64
  source_extension: Option<ExtensionId>
  source_project: Option<ProjectId>
  source_revision: Option<u64>
  formats: ordered map<FormatId, ClipboardPayload>
  total_bytes: u64

FormatId        = "<namespace>.<name>@<major>"   e.g. notwig.arranger.clips@1
ClipboardPayload = { encoding, version, bytes }  // bounded, validated
```

Built-in formats cover the current cases:

| Format | Payload |
| --- | --- |
| `notwig.arranger.clips@1` | Shared edit extract plus track references and asset hashes. |
| `notwig.audio.slices@1` | Audio slice extract plus asset hashes. |
| `notwig.midi.notes@1` | Note extract plus expression-map data. |
| `notwig.devices@1` | Device subtree snapshots, parameter values, and state references. |

A third-party extension can offer `com.example.pitch.phrase@1` without changing
the core enum. Cross-format conversion is explicit rather than implicit.

### 9.3 Contributions

`notwig.clipboard.format` is an augment point for format handlers. A handler
declares:

- the format ID and contract version;
- which selection kinds it can serialize;
- which paste targets it accepts;
- a pure serialize function from a host-provided selection snapshot;
- a paste function that validates a payload against the target and returns a
  host command batch or edit plan.

The host owns the selection snapshot and the command application. A handler
never mutates `ProjectDoc` directly and never allocates command IDs as durable
side effects; the lowering layer performs ID allocation in the app, matching the
current command/undo contract.

Converters are a separate augment point, `notwig.clipboard.converter`, with
`from` and `to` format IDs. A converter must be pure and versioned. Resolution
computes a deterministic conversion path: fewest conversions, then converter ID
order, with cycles rejected. If multiple paths tie, the host either asks the
user or applies a configured preferred target format; it does not depend on
load order.

### 9.4 Format selection and paste

Paste is a negotiation between the clipboard snapshot and the target editor:

1. The target declares its accepted formats and preferred order.
2. The host filters to formats present in the snapshot and handlers active in
   the current generation.
3. If several formats match, the target preference wins; otherwise the host
   uses the clipboard source's preferred order, then format ID order.
4. If a conversion is required, the deterministic conversion path is used.
5. If more than one equally valid interpretation remains, the host shows a
   chooser or rejects with a diagnostic. It never silently picks based on
   activation order.

This replaces the current hardcoded cross-domain rejection with explicit,
testable compatibility. For example, an audio slice clipboard can be accepted
by the arranger only when an audio-to-arranger converter is active; otherwise
the existing rejection remains.

### 9.5 Clipboard as a replaceable service

The clipboard service itself can be a single-provider slot:

```text
ServiceId: notwig.clipboard
Contract:  notwig.clipboard.service@1
Modes:     replace (one), observe (many)
Fallback:  host built-in clipboard
```

A replacement must implement the full service contract: read/write snapshots,
format negotiation, change notifications, bounded storage, and OS bridge
policy. It must not gain the ability to execute commands itself. The host
still applies plans and records undo.

Replacement is explicit. The user selects it in settings, or a trusted built-in
provider is selected by host policy. There is no "last registered wins" rule.
If the replacement fails activation or a conformance test, the host keeps the
built-in clipboard and reports the failure. Two replacement contributions for
the same service are a resolution conflict, not two active providers.

Recommendation: ship format handlers and converters first. Treat service
replacement as a later, opt-in feature for trusted providers. The built-in
clipboard stays the guaranteed path for the arranger, audio editor, MIDI editor,
and device chains.

### 9.6 Undo, assets, and OS clipboard

- Copy is not an undoable document change. Paste is one undo group through
  `Workspace::apply_batch`.
- A clipboard payload references assets by hash and project identity. Handlers
  must not embed absolute paths.
- Pasting across projects requires asset availability checks before the command
  batch is accepted. A missing asset produces a typed failure, not a partial
  paste.
- The OS clipboard bridge is host-owned. Extensions cannot read it silently.
  A host policy decides which formats may be exported as text/JSON and which
  require an explicit user action.
- The clipboard enforces total size and per-payload caps, similar to the panel
  frame limits. Decoding is bounded and happens on the UI/worker path, never on
  the audio thread.
- Observers can observe clipboard changes, but cannot cancel or rewrite the
  paste after validation.

### 9.7 Generalizing to other global services

| System | First step | Replacement later? |
| --- | --- | --- |
| Clipboard | Format handlers, converters, deterministic negotiation. | Single service slot with host fallback, trusted providers only. |
| Import/export | Augment formats and encoders/decoders. | Per-format encoder replacement is reasonable; the export orchestrator stays host-owned. |
| Analysis | Augment analysis passes and caches. | A single analysis backend may be replaceable if it implements the same caching and cancellation contract. |
| Themes/palettes | Augment theme tokens or replace a theme provider. | Low-risk replaceable presentation service. |
| Device factory | Already a composite of stock, extension, and plugin factories. | Keep the composite; do not replace the resolver with one global factory. |
| Undo/history | Observer only. | Sealed. History semantics are part of the canonical editing contract. |
| Project model and persistence | No external mutation. | Sealed for canonical data. Migration/import helpers can be contributions. |
| Audio engine and callback | Devices and analyses only at defined boundaries. | Sealed. The real-time invariants are not an extension point. |
| UI shell and focus | Contributions draw into host-defined surfaces. | Sealed until a host UI protocol exists. |

"Sealed" means contributions may observe or augment around the system but cannot
replace its canonical implementation. This is not a limitation of imagination;
it is the boundary that keeps recovery, undo, and real-time behavior sound.

## 10. Editors as extension points

The proposal says editors own their editing interfaces and define their own
points. That is feasible if "editor contribution" is split into four contracts
with different trust and UI requirements.

| Contract | Examples | Execution | External packages |
| --- | --- | --- | --- |
| Editing operation | Trim, split, quantize, transpose, pitch-correct a range. | Pure plan over a host snapshot; host lowers and applies commands. | Feasible now through typed payloads and command plans. |
| Tool/interaction mode | Scissors, velocity paint, warp marker edit. | Host-owned input gesture with editor-defined semantics. | Hard with guest panels; needs a host input/tool protocol. |
| Overlay | Automation ghost, chord labels, analysis heatmap. | Host paints a contribution-provided draw list over a host surface. | Limited to panel-local surfaces today; general overlays need a UI protocol. |
| Sub-editor | Piano roll variant, notation view, drum editor. | Separate editing surface. | Feasible through the host-owned sub-editor slot and panel surface described in section 8. The host keeps layout and focus. |

The practical first editor surfaces are the command contract and the audio
sub-editor contract from section 8. Commands can implement operations such as
humanize, quantize, or transpose over a bounded selection and return a validated
host edit family. The host applies the edit atomically and records one undo
entry. The extension never owns selection, IDs, or history.

Tools and overlays need a host UI contribution contract. The current guest panel
solution is intentionally scoped to a device card: it cannot add host widgets,
participate in host focus order, float outside the card, or expose an
accessibility tree. A general editor overlay or tool cannot be implemented by
reusing that path without a new protocol for:

- surface identity and coordinate space;
- input ownership and shortcut suppression;
- z-order and clipping;
- focus and accessibility;
- draw data validated by the host.

Sub-editor replacement is a later milestone. The shell owns panel layout, focus,
shortcuts, and the one-editor-at-a-time invariant. Replacing an editor wholesale
would require the replacement to satisfy those invariants, which is close to
replacing the shell. The better model is "the editor defines operations, tools,
and overlays; the shell remains the host".

### Editor-defined points

An editor extension can declare its own points, for example:

```text
com.example.notation.point.articulation@1
  modes: augment, observe
  payload: opaque notation articulation descriptor, max 16 KiB
```

Other extensions can contribute articulations. The notation extension consumes
them. Notwig does not need to know what an articulation is. This is real
recursive extensibility without giving the notation extension authority over
the project, audio, or UI.

## 11. Runtime and contract abstraction

The runtime model should keep the current split:

- `notwig-ext-api` owns author-facing wire types, manifests, contract metadata,
  and ABI helpers.
- `notwig-ext` owns discovery, loading, runtime adapters, and per-transport
  invocation.
- `notwig-app` owns registry composition, built-in providers, policy, and
  service slots.
- `notwig-ui` owns host UI surfaces and editor adapters, not the extension
  registry.

No Rust trait or Rust ABI should become the universal contract. In-process
built-ins can use Rust traits behind an adapter, but the contract seen by a
package is a versioned data protocol.

### Version axes

| Version | Scope | Policy |
| --- | --- | --- |
| Manifest schema | Package metadata layout | Additive major versions; old readers reject unknown majors; new readers can read old manifests. |
| Runtime ABI | Export names, wire layout, pointer protocol | Current exact-match policy can remain for the device ABI. General contracts should not reuse it blindly. |
| Extension point contract | Inputs, outputs, modes, lifecycle | Independent major/minor. A major mismatch is unresolved; a minor mismatch is accepted only if the payload is backward-compatible. |
| Service API | Host capabilities such as clipboard, selection, command plans | Independent of ABI; capability-gated and conformance-tested. |
| Package version | Author release | Informational for users; not a compatibility boundary by itself. |

`api_version: 7` in the current manifest covers the device, panel, command, and
audio sub-editor entrypoints. A platform manifest version is a separate field. This avoids
breaking every installed device package just to add registry metadata.

### Capabilities

Capabilities should be declared per contribution, not only per package. A
package may contain a device and a clipboard converter with different
capability needs. Trust itself remains a property of the loaded transport (WASM
or native), not a per-contribution claim. The host grants only the imports and
host-service handles that the specific contribution needs. Package-level
capabilities can remain as an upper bound for compatibility, but per-contribution
declarations are the safer model.

## 12. Performance

The platform adds work at four points: discovery, resolution, activation, and
invocation. They have very different costs.

| Path | Expected cost | Rule |
| --- | --- | --- |
| Discovery | Manifest reads, parsing, hashing. O(packages). | No code execution, no module compilation. |
| Resolution | Graph construction and sorting. O(points + contributions). | Startup/install only; cached by generation hash. |
| Activation | WASM compile/instantiate, native library load, provider construction. | Off the UI thread when possible; never on the audio thread. |
| Clipboard/edit invocation | Serialization and validation on a user action. | Bounded payloads; UI/worker thread; one undo group. |
| Device DSP | Existing per-block WASM copy or native in-place processing. | No extra generic hop per sample. |
| UI contribution | Draw data validation and compositing per visible contribution. | Cache resolved sets; no per-frame manifest parsing or allocation loops. |
| Audio middleware | Must not exist per sample. | Compile into the plan or run at block boundaries. |

### Compiled-in built-ins

Compiling built-ins into the same binary removes FFI and serialization from
their calls. The performance question is not "does a function call cost
anything?" but "where is the call and how often?".

- If a built-in provider is resolved into an engine plan before playback, the
  audio callback sees the same processor object as today. There is no registry
  lookup, no string comparison, and no dynamic capability check per block.
- If a built-in UI contribution is resolved into a cached set per frame, the
  cost is comparable to the current renderer branches. The danger is a design
  that re-resolves contributions or deserializes metadata every frame.
- If a contribution is a `dyn Trait` call at a block boundary, the indirect
  call is negligible compared with the DSP work. If it is called per sample,
  it is the wrong contract for that path.
- Binary size and compile time increase with more compiled-in providers. Using
  trait objects, feature gates, and moving creative devices to a bundled pack
  keeps the core lean. The current release profile uses thin LTO and one codegen
  unit, so large additions are visible in build time.
- Built-ins in the same binary cannot be unloaded or partially upgraded. That is
  acceptable for the guaranteed floor (project model, undo, stock Chain/Tool/EQ,
  shell), and it is why those should not be ordinary removable packages.

The safe performance rule is: resolve once, cache, and never consult the
registry on the real-time path. That rule matters more than whether the
implementation is compiled in, WASM, or native.

### Measurement gates

Before accepting a platform milestone, add:

- a resolver benchmark for a few hundred contributions;
- a cold-start benchmark covering discovery, resolution, and activation;
- an audio callback allocation/lock check with several providers active;
- a frame-time check for a busy editor with several overlays;
- a clipboard round-trip benchmark with the largest allowed payload;
- a regression comparison against the current device-only path.

Failing a real-time or frame-time gate blocks the milestone regardless of
feature completeness.

## 13. Maintainability

A contribution registry is maintainable when contracts are few, owned, and
tested. It becomes unmaintainable when every internal function becomes a point.

### Benefits

- One composition path for built-ins, bundled packs, and third-party packages.
- New formats, operations, and analyses do not require a new enum variant and a
  cascade of match arms.
- Resolution is a pure, testable function; load-order bugs become reproducible
  test failures.
- Built-ins exercise the same contracts, so gaps are visible instead of being
  hidden behind special cases.
- Failure isolation is explicit: a missing extension marks unresolved
  contributions instead of breaking the document or the engine.

### Costs

- Every contract is a compatibility promise. Changing a payload requires a
  version, docs, tests, and a migration story.
- The resolver, activation generation, and diagnostics are new subsystems with
  their own bugs.
- Recursive extension-defined points can create coupling between package
  authors that the host cannot understand semantically.
- Debugging spans package boundaries. A stack trace may stop at a WASM trap or
  an unresolved contribution.
- Built-ins and external packages can drift if they do not share conformance
  tests.

### Controls

- Start with two or three real contracts: device (already present), clipboard
  formats, and pure editor operations. Do not create points speculatively.
- Assign a contract owner for each `notwig.*` point. An owner reviews schema
  changes and compatibility decisions.
- Publish a conformance kit: the same tests run against the built-in provider,
  a WASM provider, and a native provider.
- Keep a registry inspector in the app that shows the resolved generation,
  contribution order, disabled reasons, and activation failures.
- Generate reference docs from contract metadata, and keep a short author guide
  for each point.
- Deprecate before removing. Support at least one previous contract major while
  the ecosystem is small.
- Put generic registry types in the lowest crate that needs them; keep
  transport-specific code in `notwig-ext` adapters. Do not make `notwig-core`
  depend on a runtime or registry I/O.

## 14. How loose can the contract be?

There are three different things that can be "loose", and they should not have
the same answer.

### 14.1 Manifest and registry envelope

This can be quite loose. A generic contribution envelope with point ID,
contract ID/version, mode, contribution ID, dependencies, capabilities, and a
bounded payload lets new points appear without core code changes. The host
validates structure, namespaces, ownership, versions, sizes, and capability
requests. This is what makes recursive extension points practical.

### 14.2 Extension-defined payloads

These can be opaque or self-described. An extension-owned point can declare a
contract and let producers/consumers agree on a JSON, postcard, or opaque byte
payload. The host does not need to understand the semantics. It enforces:

- namespace ownership;
- version presence and compatibility at the envelope level;
- size limits;
- origin and provenance;
- that the payload cannot grant authority by itself.

Opaque payloads are the right tool for notation articulations, custom analysis
metadata, or extension-specific clip annotations. They are not the right tool
for host services that touch canonical state.

### 14.3 Host services

These should stay typed and narrow. A "call any host method by name with a JSON
blob" API looks flexible but is harder to version, validate, secure, and debug.
It also makes it easy to leak authority accidentally. Host services should be
explicit capability handles with typed operations.

The practical answer is:

- **Yes**, an extension can add a new extension point and accept contributions
  without modifying the core, provided it only needs capabilities the host
  already exposes.
- **Yes**, an extension can consume those contributions through the generic
  registry and event/query APIs.
- **No**, adding a new host capability, a new canonical edit operation, a new
  audio thread callback, or a new host UI surface still requires host code.
  The registry can carry the metadata, but someone must implement and validate
  the authority side.

The recommended policy is "closed envelope, open payloads": the host understands
the envelope and the modes; the point owner understands the payload; host
services remain typed.

## 15. Migration risks and benefits

### 15.1 Benefits

- A real route for third parties to add editors, tools, formats, analysis, and
  creative devices without a core release.
- Deterministic composition and testable resolution instead of ad hoc load
  order.
- One model for built-ins and packages, which exposes ABI gaps earlier.
- Selective replaceability of non-canonical services, with a guaranteed
  fallback.
- Better failure isolation: missing or broken contributions become unresolved
  markers, not document corruption.
- A stronger ecosystem story without requiring native code for every extension.

### 15.2 Risks

| Risk | Severity | Likelihood | Mitigation |
| --- | --- | --- | --- |
| Premature abstraction creates points nobody uses | High | High | Start with three concrete contracts and a consumer for each. Delete unused points. |
| Audio or UI regression from generic dispatch | High | Medium | Resolve at plan/build time; forbid per-sample and per-frame resolution; benchmark gates. |
| Load-order bugs survive in constructors or lazy globals | High | Medium | Prohibit ambient initialization; transactional activation; shuffle-order tests. |
| Contract/version matrix becomes unmaintainable | High | Medium | Separate version axes; support one previous major; generate docs; conformance kit. |
| Built-ins become optional and break bootstrap | High | High | Core providers stay compiled-in, trusted, enabled, and non-removable; packs are additive. |
| Security escalation through new capabilities | High | Medium | Per-contribution capabilities; host-mediated project/UI/OS access; no direct command application. |
| Recursive points create hidden coupling | Medium | Medium | Namespaced ownership, opaque payload limits, cycle rejection, no new authority. |
| Existing ABI packages break | Medium | High if rushed | Keep the device ABI and manifest path; bump the ABI deliberately and reject mismatched packages instead of misreading them. |
| Command context becomes stale or leaks more state than needed | High | Medium | Bounded context snapshot, document revision check before apply, host query service for larger reads, no full document. |
| Command palette and keybindings depend on activation order | Medium | Medium | Resolve commands into the generation, use stable command IDs, deterministic conflict policy, and context expressions. |
| Command handler blocks or exceeds its UI budget | High | Medium | Short UI budget, WASM fuel limits, long work through host jobs, no audio-thread execution. |
| Audio sub-editors fake canonical data in extension state | High | High | Separate UI from processing; only host-modeled data may affect playback; explicit data contract or device route. |
| Pan/Gain envelopes are promised before the model exists | High | High | Ship Gain over existing `gain_db` first; add Pan/envelope fields, commands, engine path, and persistence as separate work. |
| Panel focus, lifecycle, or shortcuts regress | High | Medium | Host-owned chrome and focus, reused frame validation, headless interaction tests, invalid-frame fallback. |
| Undo/clipboard semantics drift | High | Medium | All mutations through commands; handlers return plans; one undo group; cross-project tests. |
| Debugging across package boundaries | Medium | High | Registry inspector, typed errors, per-generation logs, unresolved diagnostics. |
| Binary size and compile time grow | Low | Medium | Keep core minimal; move creative providers to packs; avoid monomorphized registries. |
| Dual system drift (special-case built-ins vs contributions) | Medium | High | Built-in providers implement the same contracts; parity tests; delete special-case branches after migration. |

### 15.3 Migration options

| Option | Description | Recommendation |
| --- | --- | --- |
| A. Big-bang Eclipse model | Replace devices, editors, clipboard, undo, project, and engine with points at once. | Reject. Too much contract surface before validation. |
| B. Additive contribution platform | Keep the current app and ABI. Add a registry, manifest v2, the command/palette, panel, and audio sub-editor points, built-in adapters, and later service slots for non-canonical systems. | Recommend. |
| C. Stay device-only | Keep the current host and add no general points. | Incomplete for the stated platform goal, but a valid fallback if the platform work cannot be staffed. |

### 15.4 Phased plan

| Phase | Scope | Exit criteria |
| --- | --- | --- |
| 0. Baseline | Document the contribution model, IDs, modes, version axes, command context, panel surface, and sealed core list. Keep the device ABI stable. | Design reviewed; no code change required. |
| 1. Registry and commands | Manifest v2 with `requires`, `provides`, `contributes`. Pure discovery/resolution. Add `CommandId`, resolved command descriptors, `CommandContext`, host edit families, palette, and string-keyed shortcuts. Convert built-ins incrementally. | A built-in command and an extension humanize command both run from the palette and a shortcut; context gating and stale-revision rejection are tested; one undo entry per command. |
| 2. Panels | Generalize the device panel frame into a host-owned panel surface. Add `notwig.panel`, panel tabs/docking, guest and host-rendered panels, focus, shortcut suppression, and lifecycle. Keep device panels on the device wire format. | Headless render, focus, invalid-frame fallback, multiple instances, close/reopen, and disabled-extension behavior are tested. |
| 3. Audio sub-editors | Replace the `"AUDIO EVENTS"` label with the host selector. Add `notwig.audio_editor.sub_editor`, the built-in Audio Events contribution, and the shared curve editor. Add a Gain sub-editor over existing clip/slice gain. **Status:** host selector, Audio Events, Gain, and extension-rendered sub-editors are implemented; the example package contributes a Clip Pan editor registered under its contribution id. | Default lane and extension lane switch correctly; edits apply as one undo batch; extension edits are revision-checked; time mapping, playhead, snap, waveform underlay, and focus remain host-owned. |
| 4. Canonical sub-editor data and custom editors | Add the canonical fields and engine paths needed for Pan and time-varying Gain, or explicitly scope them out. **Status:** Pan and time-varying gain curves are canonical; custom sub-editors use the panel surface. | Canonical data round-trips and affects rendering; no hidden playback state; custom sub-editor context is bounded and stale-checked. |
| 5. Clipboard formats | Add format handlers and converters for arranger, audio, MIDI, and devices. Replace the closed `Clipboard` enum with a host snapshot plus built-in handlers. | Cross-domain paste tests, asset-missing tests, undo grouping, OS marker compatibility. |
| 6. Single-provider services | Introduce service slots for low-risk systems (theme, export encoder, later clipboard replacement) with host fallback and conformance tests. | Replacement failure falls back; no load-order selection; conformance pass. |
| 7. Recursive ecosystem | Allow extension-defined points for extension-to-extension contracts. Add diagnostics and author docs. | Namespace enforcement, missing-owner behavior, payload limits, sample ecosystem test. |
| 8. Optional runtimes | WIT/component model or out-of-process extensions for higher isolation. | Same contracts pass over the new transport; no audio-path regression. |

Phases 1-4 are the first migration. Phase 1 (commands, palette, bindings,
context, host edit families, WASM/native/browser execution, and the example
humanize command) is implemented. They are sequenced so each one is useful on
its own: commands do not need panels, panels do not need audio sub-editors, and
the shared curve editor does not need custom pitch-correction processing. Phase
3 can ship with existing gain data; phase 4 must not pretend that a missing Pan
field or hidden extension state already works. Phases 5-8 are follow-ups, not
prerequisites.

### Rollback

The platform must be shippable behind a build/runtime flag during migration.
The built-in code path remains the fallback. If resolution or activation fails,
the app starts with the guaranteed core and reports which package/generation
failed. A project must remain loadable even if every non-core contribution is
disabled.

## 16. Invariants and test plan

The following invariants are part of the design, not implementation details:

1. Discovery never executes extension code.
2. Resolution is a pure function of declared inputs and has a deterministic
   generation hash.
3. At most one active version of an extension ID exists.
4. One replacement provider is selected explicitly; duplicates are a conflict.
5. `before`/`after` cycles are rejected.
6. Activation is atomic; a failed generation never becomes visible partially.
7. Audio callbacks never resolve contributions, load modules, or run generic
   middleware per sample.
8. Extensions cannot mutate `ProjectDoc`; all changes go through validated
   commands and app-owned history.
9. Capabilities are enforced per contribution, not just per package.
10. Missing points, owners, dependencies, and assets produce typed unresolved
    states, not panics or silent corruption.
11. Built-in providers implement the same contracts they expose.
12. User-visible ordering comes from declared keys, not discovery order.
13. Command effects use host-owned edit families; a command cannot submit an
    arbitrary `CommandPayload`.
14. A command snapshot is checked against the current revision before its effect
    is applied; stale contexts fail atomically.
15. Audio sub-editors cannot store playback-affecting state outside a
    host-modeled, versioned location.
16. General panels draw only inside their host surface; they do not create host
    chrome, focus order, or windows.

| Test | Level | What it proves |
| --- | --- | --- |
| Shuffle discovery/install order and compare resolved generation hash | Registry | Load-order independence. |
| Permute contribution order within a point | Registry | Stable augment/middleware ordering. |
| Duplicate extension IDs and duplicate replacements | Registry | Explicit conflict handling. |
| Missing dependency, optional dependency, disabled dependency | Registry | Deterministic unresolved behavior. |
| Requirement and middleware cycles | Registry | Rejection without partial activation. |
| Activation failure keeps the previous generation | Registry/app | Transactional activation. |
| Core providers and external providers pass the same contract kit | Conformance | Built-in parity. |
| Built-in and extension commands appear in the palette and run from a shortcut | UI/app | Command registry, context gating, and dynamic bindings. |
| Command context changes before apply | App | Stale-revision rejection with no document mutation. |
| Extension command edits selected MIDI notes | App/UI | Context selection, host edit family, one undo entry, editor refresh. |
| Conflicting keybindings across built-ins and extensions | UI/config | Deterministic conflict handling without load order. |
| Audio sub-editor switches while preserving clip time and selection | UI | Host-owned slot and shared timeline. |
| Shared curve sub-editor applies and undoes one batch | UI/app | Curve widget, host data binding, undo grouping. |
| Custom sub-editor receives a bounded context and a stale edit fails | App/UI | Sub-editor context and command effect boundary. |
| Guest panel focus, shortcut suppression, and invalid-frame fallback | UI | General panel surface lifecycle. |
| Extension disabled while its panel or command is open | App/UI | Deterministic fallback without corrupting state. |
| Clipboard write/paste for every built-in format and converter | Core/app | Format negotiation and undo grouping. |
| Clipboard across projects with missing assets | App | Typed failure without partial command application. |
| Audio callback under contribution load | Engine | No allocation, lock, registry lookup, or module call. |
| Headless editor overlay render and focus | UI | Host UI contract before shipping overlays. |
| Manifest fuzz and size/path limits | Registry | Robustness against malformed packages. |
| Project load with all non-core contributions disabled | App | Guaranteed core and recovery path. |

## 17. Non-goals and open questions

Non-goals for the first platform milestone:

- Replacing `ProjectDoc`, `apply`, undo, or the canonical revision model.
- Replacing the audio callback, transport, or prepared-runtime contract.
- Arbitrary host UI from sandboxed extensions.
- Loading untrusted native libraries.
- A universal scripting language or dynamic host-method dispatch.
- Arbitrary command result payloads outside the host edit families.
- Pan/Gain envelopes or pitch correction that affect playback before canonical
  data and engine paths exist.
- Free-floating guest windows or host focus/accessibility integration in the
  first panel surface.
- Making stock Chain, Tool, or EQ removable packages. The bootstrap and offline
  render arguments in [`plan/MOVING_DEVICES.md`](../../plan/MOVING_DEVICES.md)
  still apply.
- A marketplace or remote execution model.

Open questions:

- Should extension-defined points require a machine-readable schema, or is an
  opaque payload with a version enough for the first ecosystem?
- Which service is the first single-provider slot worth shipping (theme,
  export encoder, or clipboard) and what is its conformance suite?
- Should the registry live in `notwig-ext` or a small pure resolver crate that
  both the app and CLI render path can use?
- How are built-in manifests represented for introspection without installing
  packages: embedded JSON, generated code, or a static provider table?
- Which audio sub-editor is the first custom proof after Audio Events and Gain:
  pitch correction, spectral repair, or a simpler waveform tool?
- Which data/processing route does pitch correction take: host extension data
  plus a processor, a destructive asset render, or a device?
- How long is the contract deprecation window before the ecosystem is large
  enough to justify a formal policy?

## 18. Direct answers

- **Feasible?** Yes, as an additive contribution platform with a guaranteed
  core. No, as a one-step replacement of every subsystem.
- **First migration?** Dynamic commands with a palette, context, and string
  keybindings; host-owned audio sub-editors with a shared curve editor and a
  custom panel slot; and shell-hosted extension panels. Built-ins become normal
  providers where practical.
- **Commands?** A bounded `CommandContext` snapshot plus host-owned edit
  families. Commands can read the selection, return validated edits, update
  focus/selection, and run from the palette or a shortcut. They cannot submit
  arbitrary document commands or run on the audio thread.
- **Audio sub-editors?** The host owns the `"AUDIO EVENTS"` button, slot, time
  mapping, focus, and undo. Extensions contribute shared-curve targets or custom
  panel sub-editors. Gain can start over existing `gain_db`; Pan and pitch
  correction need canonical data and a processing route before they can affect
  playback.
- **Panels?** The host owns docking, tabs, focus, resize, and close. Guest panels
  reuse the validated `PanelFrame` transport inside a bounded surface; simple
  panels can use a host-rendered schema. Panels compose with commands.
- **Clipboard?** Host-owned snapshot with format handlers, deterministic
  negotiation, converters, and command-plan pastes. A single-provider clipboard
  service slot is possible later, with the built-in as fallback and explicit
  selection. Copy stays outside undo; paste is one undo group.
- **Packaging with order-independent behavior?** One active version per ID,
  lockfile-pinned packs, pure resolution, explicit ordering keys, stable
  tie-breaks, cycle rejection, and atomic activation. Load order is never a
  semantic input.
- **Compile-time performance?** No audio-path cost when providers are resolved
  at plan/build time. The costs are binary size, compile time, and startup
  activation. Per-sample middleware and per-frame resolution are forbidden.
- **Maintainable?** Yes with few contracts, contract owners, conformance tests,
  diagnostics, and deprecation rules. No as a broad platform without those
  controls.
- **How loose?** Closed envelope, open extension-defined payloads, typed host
  services. New extension-to-extension functionality can avoid core changes;
  new host authority and new host UI surfaces cannot.
- **Risks and benefits?** The benefits are real extensibility and deterministic
  composition. The risks are contract sprawl, real-time/UI regressions,
  bootstrap breakage, and version-matrix cost. Phases 1-4 capture most of the
  value with the least irreversible risk.
