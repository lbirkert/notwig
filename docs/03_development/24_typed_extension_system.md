# Typed extension system

Status: phase foundation implemented; typed registry still proposed. This page
replaces the earlier message-based sketch. The target is a flat, typed,
registry-backed extension system with macro-generated RPC proxies over the
existing WASM/native/browser transports. The preload/load/unload builders,
macros, ABI exports, host-wide preload ordering, runtime point/command/
sub-editor/bottom-panel registration manifests, native/web host resolution,
stock/example migration, scaffold, and the core + package bottom-panel registry
are in place.
Typed interface proxies, handle calls, replacement, `Single` cardinality, and
user load-order resolution remain to be implemented.

Related: [`Extension Platform Architecture`](23_extension_platform_architecture.md),
[`Extension ABI and Hosts`](19_extension_abi_and_hosts.md), and
[`Extension Panels`](12_extension_panels.md).

## 1. Invariants

1. **Interface and extension point are independent.** `dyn Panel` is one
   interface; `TRACK_PANEL` and `EFFECT_PANEL` are different registration scopes
   that both accept `Panel`.
2. **IDs are stable ABI identifiers.** Interface ids, extension point ids,
   extension ids, and implementation ids are namespaced strings, never Rust
   `TypeId`.
3. **The registry is flat.** Ownership creates logical relationships; there is
   no runtime tree.
4. **Every implementation has an identity.** An extension may register several
   implementations of one interface, so routing by extension id alone is not
   enough.
5. **No Rust references or trait objects cross a package boundary.** Generated
   typed proxies serialize requests and deserialize replies.
6. **Registration ownership belongs to the contributing extension.**
   Extensions cannot delete another extension's rows. Replacement is an explicit
   override edge between stable implementation/extension ids, resolved by the
   host with user-controlled extension load order as the tie-breaker.
7. **Discovery order does not matter.** Declarations and contributions are
   collected first, then resolved as one graph. The user-controlled extension
   load order is explicit input, not filesystem or installation order.
8. **The context is the author-facing API.** Extensions do not manage
   transports, registries, WASM instances, or locks.
9. **Real-time audio is separate.** General RPC never enters the audio callback.

## 2. Naming and core ownership

Extension-point constants use the `EP_` prefix; `EP` means **extension point**.
The id stays a stable namespaced string, while the constant name makes call
sites read as point registrations:

- `EP_NOTWIG_DEVICES` -> `notwig.devices`
- `EP_COMMANDS` -> `notwig.commands`
- `EP_NOTWIG_AUDIO_EDITOR_SUB_EDITOR` -> `notwig.audio_editor.sub_editor`
- `EP_NOTWIG_BOTTOM_PANEL` -> `notwig.bottom_panel`
- `EP_NOTWIG_CLIPBOARD` -> `notwig.clipboard`

Built-in implementations that ship with the application are registered under
the fixed, non-uninstallable extension owner `notwig.core` / `Notwig Core`.
They use the same points and registry shape as package extensions; the only
difference is ownership and installation policy. The device, MIDI, and audio editor
implementations share `EP_NOTWIG_BOTTOM_PANEL` and are distinguished by stable
implementation ids (`notwig.core.device_editor`, etc.). Package extensions
register arbitrary ids such as `com.example.notes` on the same point. The shell
keeps at most one bottom panel open and resolves it by id instead of a
hard-coded match.

The clipboard is a single replaceable provider point. `ClipboardProvider` owns
the structured `notwig_core::Clipboard` payload and cached platform text. The
core provider is the default; replacing it does not change editor call sites.
The shell selects exactly one active provider.

Host-side typed points use `CoreExtensionRegistry`. `get_extensions(EP_...)`
returns all active implementations for a `Many` point. `get_extension(EP_...)`
returns the single active implementation for a `Single` point and panics on
zero or multiple active implementations. `try_get_extension` is the recoverable
form. `register` adds an implementation; `replace` installs and activates a
replacement; `set_active` selects among existing implementations.

Extension crate roots follow the same separation: `lib.rs` contains wiring and
the lifecycle entrypoints only. Implementations and helpers live in modules,
tests live under `tests/`, and `load`, `unload`, and any explicit reload hook
are the last items in the file.

## 2. Core model

| Concept | Example | Uniqueness |
| --- | --- | --- |
| Interface | `dyn Panel`, `com.notwig.api.panel@1` | One stable interface id |
| Extension point | `TRACK_PANEL`, `com.notwig.track.panel` | One stable point id |
| Extension | `com.example.midi-tools` | One package identity |
| Implementation | `MidiMenu` registered to `TRACK_PANEL` | One registration identity |

One interface can be used by any number of points. One point can contain any
number of implementations. One extension can implement multiple interfaces and
contribute to multiple points.

```rust
#[notwig_ext_trait(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(
        &self,
        context: &ExtensionContext<'_>,
        input: PanelInput,
    ) -> Result<PanelFrame, ExtensionError>;
}

pub const TRACK_PANEL: ExtensionPoint<dyn Panel> =
    ExtensionPoint::new("com.notwig.track.panel");
pub const EFFECT_PANEL: ExtensionPoint<dyn Panel> =
    ExtensionPoint::new("com.notwig.effect.panel");
pub const PROJECT_PANEL: ExtensionPoint<dyn Panel> =
    ExtensionPoint::new("com.notwig.project.panel");
```

```rust
let track_panels = context.get_extensions(TRACK_PANEL);
let effect_panels = context.get_extensions(EFFECT_PANEL);
let project_panels = context.get_extensions(PROJECT_PANEL);
```

`ExtensionPoint<T>` is only a typed key:

```rust
pub struct ExtensionPoint<T: ?Sized> {
    id: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> ExtensionPoint<T> {
    pub const fn new(id: &'static str) -> Self { /* ... */ }
    pub const fn id(&self) -> &'static str { self.id }
}
```

It contains no runtime, implementations, instance handles, or heap state.

## 3. Author-facing API

### 3.1 Interface macro

`#[notwig_ext_trait]` declares a callable interface and generates the transport
plumbing:

```rust
#[notwig_ext_trait(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(&self, input: PanelInput) -> Result<PanelFrame, ExtensionError>;
}
```

Generated items:

- `PanelHandle` with the same method surface;
- `PanelRequest`, `PanelResponse`;
- `PanelDispatcher` for the implementation side;
- `ExtensionCall` via `#[derive(ExtensionCall)]` and `#[extension_call(Panel)]`;
- `ExtensionTrait` for `dyn Trait`, with `type Handle = TraitHandle`;
- scheduler-based handle construction through `ExtensionTrait::create_handle`;
- interface descriptor, method ids, schema ids, and version metadata.

Registration takes the actual instance:

```rust
extension.register_extension(TRACK_PANEL, MyTrackPanel::new());
```

The guest owns the instance as `Box<dyn ExtensionCall>`; only postcard payloads
and stable ids cross the WASM/native boundary. The scheduler selects the object
by index and calls `ExtensionCall::process` with the method id and payload.

The developer writes the normal trait impl and annotates the concrete type so
the guest can register the instance:

```rust
#[derive(ExtensionCall)]
#[extension_call(Panel)]
struct MyTrackPanel;
    fn draw(
        &self,
        context: &ExtensionContext<'_>,
        input: PanelInput,
    ) -> Result<PanelFrame, ExtensionError> {
        // Build a child panel frame; resolve other points from context.
    }
}
```

Methods receive `&ExtensionContext<'_>` so implementations can resolve other
points through `get_extensions`, `get_extension`, and `try_get_extension`.

The generated handle is a concrete proxy, not a Rust trait object:

```rust
let frame = panel_handle.draw(&context, input)?;
```

Macro restrictions for the first milestone:

- `&self` plus a required `&ExtensionContext<'_>` argument;
- owned, serializable arguments and return values;
- explicit `Result<_, ExtensionError>` errors;
- no borrowed values, references, or lifetimes across the boundary;
- no generic methods or associated types;
- no host pointers or `egui::Ui`;
- synchronous UI/metadata calls only.

The exact generic-bound/orphan strategy for
`register_extension_factory::<MyPanel>(TRACK_PANEL)` must be proven first with a small
macro proof of concept. The target syntax is fixed; the generated adapter
machinery is allowed to be hidden.

### 3.2 Lifecycle entrypoints

`#[notwig_extension]` is replaced by three phase-specific attributes:

- `#[notwig_ext(preload)]` declares extension points and other packages-level
  contracts. All point declarations happen here, before any implementation can
  register to them.
- `#[notwig_ext(load)]` registers implementations, commands, devices,
  sub-editors, and other runtime contributions.
- `#[notwig_ext(unload)]` releases extension-owned resources and state.

One binary may define at most one function for each phase. `load` is required
because it generates the main contribution ABI. `preload` and `unload` are
optional, but a package that owns extension points must provide `preload`.

```rust
// Point owner: com.notwig.track
#[notwig_ext(preload)]
pub fn preload(extension: &mut PreloadBuilder) {
    extension.register_extension_point(TRACK_PANEL);
}

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension_factory::<TrackDevice>(EP_NOTWIG_DEVICES);
}

#[notwig_ext(unload)]
pub fn unload(_context: &mut UnloadContext) {
    // Release extension-owned resources; the host owns instance teardown.
}
```

```rust
// Contributor: com.example.midi-tools
#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    // Point already exists from the preload phase.
    extension.register_extension_factory::<MidiMenu>(TRACK_PANEL);
}
```

The phase builders enforce the separation at compile time:

- `PreloadBuilder` exposes point declaration only;
- `ExtensionBuilder` exposes implementation and existing contribution
  registration, but not new point declarations;
- `UnloadContext` marks the unload phase boundary; the host owns instance
  teardown after the callback returns.

This removes the single-entrypoint restriction while keeping one binary per
extension package. The old `#[notwig_extension]` form is kept as an alias for
`#[notwig_ext(load)]` during migration and can be removed in a later ABI bump.

### 3.3 Registration

```rust
// Point owner, in #[notwig_ext(preload)].
extension.register_extension_point(TRACK_PANEL);

// Contributor, in #[notwig_ext(load)].
extension.register_extension_factory::<MyPanel>(TRACK_PANEL);
```

Ownership is implicit in the extension being initialized. The runtime records:

```text
TRACK_PANEL
  owner: com.notwig.track
  implementations:
    MyPanel -> com.example.midi-tools
```

Bulk replacement affects only the caller's registrations:

```rust
extension.set_extensions::<dyn Panel>(
    TRACK_PANEL,
    [MyPanel::registration(), AnotherPanel::registration()],
);

extension.clear_own_extensions(TRACK_PANEL);
```

### 3.4 Replacing an implementation

Some points want a single active provider, for example a clipboard backend. An
extension may offer a replacement without deleting the previous owner's row:

```rust
pub enum RegistrationMode {
    /// Add another implementation.
    Augment,
    /// Hide the named implementation while still preserving its ownership row.
    Replace { target: ImplementationId },
}

pub struct RegistrationOptions {
    pub mode: RegistrationMode,
    pub priority: i32,
    pub before: Vec<ExtensionId>,
    pub after: Vec<ExtensionId>,
}

extension.replace_extension::<MyClipboard>(
    CLIPBOARD_BACKEND,
    target_implementation_id,
);
```

The registry stores a directed replacement edge. Resolution removes the target
from the active set while keeping all ownership rows intact. This gives the
desired hot-swap behavior without letting one extension mutate another's
registrations.

Load-order independence rules:

- replacement targets are stable `ImplementationId`s, not load indices;
- a missing target is resolved after the complete graph is collected;
- replacement chains are followed transitively;
- replacement cycles are a resolution error;
- when two implementations replace the same target, the later extension in the
  user load order wins; the other remains inactive with a diagnostic;
- the point owner can allow or reject replacement through its point policy.

For a user-facing provider swap, the host can also select an active provider
directly:

```rust
runtime.set_active_implementation(CLIPBOARD_BACKEND, implementation_id);
```

That selection is UI-thread state and is independent of discovery order.

### 3.5 Extension load order

Duplicate replacements need one deterministic winner. Discovery order is not
good enough because it can change with filesystem iteration, install time, or
platform. The host instead owns a persistent, user-editable extension load
order:

```rust
pub struct ExtensionLoadOrder(Vec<ExtensionId>);

impl ExtensionRuntime {
    pub fn set_extension_load_order(&mut self, order: ExtensionLoadOrder);
    pub fn extension_load_order(&self) -> &[ExtensionId];
}
```

Rules:

- the list contains enabled extensions, ordered from earliest to latest;
- default order is built-ins first, then extensions in stable id order; new
  installs append;
- Settings > Extensions lets the user move entries up or down;
- the order is persisted by extension id, not install directory;
- changing the order bumps registry generation and recomputes the resolved graph;
- discovery order never affects the result.

For duplicate replacements, **later in the load order wins**. The losing
replacement stays registered but inactive and produces a diagnostic. This makes
provider exchange predictable and user-reversible. Explicit
`set_active_implementation` still overrides automatic replacement selection.

Load order is also the final tie-breaker for ordered points after `before` /
`after` constraints and explicit priority.

### 3.4 Context and handles

```rust
impl ExtensionContext {
    pub fn get_extensions<T: ?Sized>(
        &self,
        point: ExtensionPoint<T>,
    ) -> Vec<ExtensionHandle<T>>;

    pub fn get_extension<T: ?Sized>(
        &self,
        point: ExtensionPoint<T>,
    ) -> ExtensionHandle<T>;

    pub fn try_get_extension<T: ?Sized>(
        &self,
        point: ExtensionPoint<T>,
    ) -> Result<ExtensionHandle<T>, ExtensionError>;

    pub fn get<T: ?Sized>(
        &self,
        extension: ExtensionId,
    ) -> Result<ExtensionHandle<T>, ExtensionError>;
}
```

`get_extensions` returns an ordered snapshot of active handles. No registry
reference or lock survives the query.

`get_extension` is the single-provider convenience. It **panics** when the point
has zero active implementations or more than one. `try_get_extension` returns a
typed error instead and is preferred inside host UI and in recoverable paths.
`get` is explicit service lookup and does not infer a point from the trait.

`get_extension` is most useful when the point declares a single-provider
cardinality policy:

```rust
pub enum Cardinality {
    Many,
    Single,
}
```

The runtime validates `Single` points at resolution: a point may have many
registered rows and replacements, but it must resolve to exactly one active
implementation. Adding a second active provider without a replacement edge is a
resolution error.

```rust
pub struct ExtensionHandle<T: ?Sized> {
    implementation_id: ImplementationId,
    generation: u64,
    runtime: RuntimeHandle,
    marker: PhantomData<fn() -> T>,
}
```

A handle routes to one implementation of one loaded generation. If the target
is unloaded or reloaded, the call returns `ExtensionUnavailable` or
`StaleHandle`; it never touches freed memory.

## 4. Runtime architecture

### 4.1 Flat registry

```rust
pub struct ExtensionRegistry {
    points: BTreeMap<ExtensionPointId, ExtensionPointDescriptor>,
    implementations:
        BTreeMap<ExtensionPointId, Vec<ImplementationDescriptor>>,
}

pub struct ImplementationDescriptor {
    pub id: ImplementationId,
    pub extension_id: ExtensionId,
    pub interface_id: InterfaceId,
    pub interface_version: u32,
    pub dispatcher: DispatcherHandle,
    pub options: RegistrationOptions,
}
```

Resolution is two-phase:

1. collect point declarations and implementation contributions from every
   loaded extension;
2. validate the graph: point exists, interface matches, contract major matches,
   ids are unique, owners are enabled, replacement edges are acyclic, and
   ordering constraints are acyclic.

A missing point is reported after resolution, not at first registration. Missing
replacement targets and replacement cycles are typed resolution errors.
Conflicting replacements of one target are resolved by the user-controlled
extension load order: later wins, earlier losers stay inactive with diagnostics.
`Single` points must still resolve to exactly one active implementation after
replacement and explicit provider selection. Filesystem/discovery order never
changes the result.

### 4.2 Ownership and lifecycle

- Every registration belongs to its contributing extension generation.
- Handles capture `(implementation_id, generation)`.
- Reloading an extension invalidates old handles and publishes new ones.
- A disabled point owner disables its point; contributing implementations become
  unresolved and visible as diagnostics, not crashes.
- Fan-out callers receive per-implementation results:

```rust
pub struct ExtensionCallResult<T> {
    pub implementation_id: ImplementationId,
    pub result: Result<T, ExtensionError>,
}
```

### 4.3 Load lifecycle

The runtime executes phases in a fixed order:

1. Discover and instantiate enabled extensions. Filesystem order is ignored.
2. Call every `preload` in user extension load order. Collect point declarations
   and interface/version metadata.
3. Validate the point graph: duplicate ids, namespace ownership, interface ids,
   and contract majors. A missing point is not reported here yet.
4. Call every `load` in user extension load order. Collect implementations,
   commands, devices, sub-editors, and other contributions.
5. Resolve the complete contribution graph, including replacements, ordering,
   cardinality, and missing points.
6. Activate the new registry generation. Existing handles from older generations
   become stale.
7. On disable, reload, or shutdown, invalidate handles first, then call `unload`
   in reverse load order and remove that generation's rows.

A package that fails `preload` cannot own points and is marked failed. A package
that fails `load` is unavailable; its already-collected rows are discarded. An
`unload` failure is logged and must not block unloading other packages.

### 4.4 Threading

- Registration and resolution live on the UI thread.
- The runtime never holds a registry lock while invoking extension code.
- Queries clone owned handles, release registry access, then dispatch.
- Invocations are re-entrant-safe and bounded per frame.
- The audio path uses dedicated DSP ABIs and never calls the generic RPC path.

## 5. Transport

The public API is transport-independent. The host adds an invocation transport:

```rust
pub trait ExtensionTransport: Send + Sync {
    fn invoke(
        &self,
        target: ImplementationId,
        request: ExtensionRequest,
    ) -> Result<ExtensionResponse, TransportError>;
}
```

The wire envelope is independent of Rust type names:

```rust
pub struct ExtensionRequest {
    pub protocol_version: u32,
    pub interface_id: InterfaceId,
    pub interface_version: u32,
    pub method_id: MethodId,
    pub request_id: RequestId,
    pub payload: Vec<u8>,
}
```

Implementation plan for the current code:

- generate optional `notwig_ext_preload`, `notwig_ext_load`, and
  `notwig_ext_unload` exports from the three attributes;
- add a guest export returning bounded registration descriptors for points,
  implementations, commands, and existing contributions;
- keep the existing contribution exports during migration;
- add a guest invoke export that carries `(method_id, payload)`;
- keep existing device/command/sub-editor exports during migration;
- add a host-side `ExtensionRuntime` on the UI thread that owns the registry,
  loaded generations, dispatch, and diagnostics;
- adapt `notwig-ext`’s native/WASM/browser instances behind the transport;
- bump the ABI when the new exports land.

The existing `ExtensionHost` remains the loader/backend. The new runtime layers
on top and becomes the authority used by the app/UI. A `NotwigCommands` facade
on the UI thread exposes context lookups, command execution, revision checks,
and undo grouping to extensions and host UI.

## 6. GUI composition

Panels remain bounded child frames:

```rust
#[notwig_ext_trait(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(&self, input: PanelInput) -> Result<PanelFrame, ExtensionError>;
    fn handle_interaction(
        &self,
        input: PanelInteraction,
    ) -> Result<PanelFrameUpdate, ExtensionError>;
}
```

`PanelInput` carries available size, scale, focus/input, theme, and bounded
context. The host validates `PanelFrame` into a trusted
`ValidatedPanelFrame`. The parent decides placement, clipping, z-order, and
layout; the child owns only its own content.

The runtime records frame ownership:

```rust
pub struct FrameOwnership {
    pub frame_id: FrameId,
    pub implementation_id: ImplementationId,
    pub extension_id: ExtensionId,
}
```

Interaction routing uses ownership lookup, not widget guessing. A parent cannot
forge an interaction for an unrelated child frame.

Initial rendering replaces full frames. Incremental updates are deferred.

## 7. Examples

### 7.1 Context menu

```rust
#[notwig_ext_trait(id = "com.notwig.api.menu", version = 1)]
pub trait ContextMenuContributor {
    fn draw(
        &self,
        input: ContextMenuInput,
    ) -> Result<PanelFrame, ExtensionError>;
}

pub const TRACK_CONTEXT_MENU: ExtensionPoint<dyn ContextMenuContributor> =
    ExtensionPoint::new("com.notwig.track.context_menu");
```

Track extension declares the point. MIDI and audio extensions register menu
implementations. The track extension queries the point, calls each handle, and
composes the returned child frames in its own menu surface.

### 7.2 Recursive points

```text
Piano Roll extension
  declares PIANO_ROLL_TOOLBAR

MIDI Editing extension
  implements PIANO_ROLL_TOOLBAR
  declares QUANTIZATION_ALGORITHMS

Swing extension
  implements QUANTIZATION_ALGORITHMS
```

The MIDI extension queries its own point through the same context API. There is
no special nesting mechanism; the flat registry plus point ownership is enough.

### 7.3 Same implementation in several points

Registering `BrowserPanel` to `PROJECT_PANEL`, `TRACK_PANEL`, and `EFFECT_PANEL`
creates three independent implementation instances. Shared state is explicit
and deferred to a later registration mode.

### 7.4 Services versus points

- `context.get::<dyn Synthesizer>(id)` is an explicit single-provider lookup.
- `context.get_extension(SINGLE_POINT)` requires exactly one active provider.
- `context.get_extensions(POINT)` discovers unknown contributors.

Not every extension capability should become a point.

### 7.5 Clipboard provider exchange

`CLIPBOARD_BACKEND` is a `Single` point. The stock clipboard registers the
default provider. A clipboard extension registers an alternative and either:

- adds an explicit replacement edge to the stock implementation; or
- is selected by the user/host through
  `runtime.set_active_implementation(CLIPBOARD_BACKEND, id)`.

Both paths are load-order independent. The stock provider still owns its row and
can be reactivated later. Consumers call
`context.get_extension(CLIPBOARD_BACKEND)` and therefore cannot accidentally read
two active clipboard backends.

## 8. Ordering

Ordering policy belongs to the point descriptor:

```rust
pub enum OrderingPolicy {
    Unordered,
    Priority,
    ExplicitConstraints,
}
```

When ordered, resolution applies explicit `before`/`after`, then priority, then
the user-controlled extension load order, then stable
`(extension_id, implementation_id)` tie-breakers. Replacement edges are resolved
before ordering; ordering applies only to the resulting active set. Cycles in
either graph are resolution errors.

## 9. Integration order

Start with the smallest thing that proves the model across two genuinely
independent WASM instances.

### Milestone 0 — Proof of concept

- Add `#[notwig_ext_trait]` for one trait:
  `Test::execute(String) -> Result<String, ExtensionError>`.
- Generate handle/request/response/dispatcher.
- Define `TEST_POINT: ExtensionPoint<dyn Test>`.
- Extension A declares the point; extension B registers an implementation.
- Extension A queries the point and calls B through the host transport.
- Verify normal method-call ergonomics with no manual serialization in author
  code.

### Milestone 1 — Registry and ownership

- `ExtensionPoint<T>`, declarations, implementations, owned handles,
  two-phase resolution, registration ownership, and `set_extensions`.
- Replacement edges, `replace_extension`, single-provider cardinality,
  `get_extension` / `try_get_extension`, and persistent load order.
- Add `#[notwig_ext(preload/load/unload)]` macro exports and phase builders.
- Keep existing device/command/sub-editor exports behind adapters.
- Add registry/invoke ABI exports and the UI-thread runtime.

### Milestone 2 — Existing host points

- Map `notwig.commands`, `notwig.audio_editor.sub_editor`, and `notwig.devices` onto
  the new registry.
- Introduce `NotwigCommands` as the app/UI facade.
- Map `#[notwig_extension]` to `#[notwig_ext(load)]` with a deprecation warning for one ABI window.

### Milestone 3 — Panels and context menus

- Add the `Panel` interface and child-frame validation/composition.
- Add interaction ownership and routing.
- Use the track context menu as the first multi-contributor UI.

### Milestone 4 — Recursive points

- Implement the piano roll → MIDI editing → quantization chain.
- Prove registration order independence and cycle diagnostics.

### Milestone 5 — Lifecycle and failure

- Unload/reload, stale handles, per-implementation failures, timeout budgets, and
  re-entrant invocation tests.
- Extension load-order UI, persistence, registry recomputation, and replacement
  conflict diagnostics.

### Milestone 6 — Deferred features

- Ordering policies beyond priority.
- Async interfaces and cancellation.
- Incremental frame updates.
- Shared implementation registrations.
- Specialized audio APIs (separate track, not generic RPC).

## 10. Acceptance criteria

- Two independent WASM extensions implement and call one interface through typed
  handles.
- Multiple points use the same interface without ambiguity.
- Multiple implementations register to one point; one extension can own several.
- Discovery order is irrelevant, including under replacement chains.
- Contributors cannot delete another extension's rows; explicit replacement
  edges and host provider selection are deterministic.
- Reordering extensions changes duplicate-replacement winners predictably, and
  the order persists across restarts.
- `get_extension` panics on zero or multiple active implementations;
  `try_get_extension` returns a typed error.
- A `Single` point with multiple active providers is a resolution error.
- Generated proxies reject non-serializable and borrowed signatures at compile
  time.
- Unknown points, interface/version mismatches, duplicate ids, stale handles, and
  ordering cycles produce typed errors.
- Panel frames validate before composition; child widget ids cannot collide.
- Context menu contributions render and route interactions through the parent.
- The generic RPC path never runs on the audio callback.

## 11. Deliberate exclusions

- No use of `TypeId` as ABI identity.
- No cross-WASM trait objects or borrowed references.
- No audio processing through generic RPC.
- No hierarchical runtime registry.
- No arbitrary registration after initialization.
- No generic trait methods, associated types, or streaming in the first version.
- No automatic global service discovery without an explicit provider id.
