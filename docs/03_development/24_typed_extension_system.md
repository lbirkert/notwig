# Typed extension system

Status: proposed architecture. This page replaces the earlier message-based
sketch. The target is a flat, typed, registry-backed extension system with
macro-generated RPC proxies over the existing WASM/native/browser transports.

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
6. **Registration ownership belongs to the contributing extension.** An
   extension cannot clear or replace another extension's registrations.
7. **Registration order does not matter.** Declarations and contributions are
   collected first, then resolved as one graph.
8. **The context is the author-facing API.** Extensions do not manage
   transports, registries, WASM instances, or locks.
9. **Real-time audio is separate.** General RPC never enters the audio callback.

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
#[extension_api(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(&self, input: PanelInput) -> Result<PanelFrame, ExtensionError>;
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

`#[extension_api]` declares a callable interface and generates the transport
plumbing:

```rust
#[extension_api(id = "com.notwig.api.panel", version = 1)]
pub trait Panel {
    fn draw(&self, input: PanelInput) -> Result<PanelFrame, ExtensionError>;
}
```

Generated items:

- `PanelHandle` with the same method surface;
- `PanelRequest`, `PanelResponse`;
- `PanelDispatcher` for the implementation side;
- interface descriptor, method ids, schema ids, and version metadata.

The developer still writes a normal Rust implementation:

```rust
impl Panel for MyTrackPanel {
    fn draw(&self, input: PanelInput) -> Result<PanelFrame, ExtensionError> {
        // Build a child panel frame.
    }
}
```

The generated handle is a concrete proxy, not a Rust trait object:

```rust
let frame = panel_handle.draw(input)?;
```

Macro restrictions for the first milestone:

- owned, serializable arguments and return values;
- explicit `Result<_, ExtensionError>` errors;
- no borrowed values, references, or lifetimes across the boundary;
- no generic methods or associated types;
- no host pointers or `egui::Ui`;
- synchronous UI/metadata calls only.

The exact generic-bound/orphan strategy for
`register_extension::<MyPanel>(TRACK_PANEL)` must be proven first with a small
macro proof of concept. The target syntax is fixed; the generated adapter
machinery is allowed to be hidden.

### 3.2 Entry point

Keep the existing one-entrypoint-per-binary macro. The entrypoint accepts the
builder/runtime and may declare points and register many implementations:

```rust
#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_extension_point(TRACK_PANEL);
    extension.register_extension::<MyPanel>(TRACK_PANEL);
}
```

No second `#[extension]` macro is needed. `Extension` grows into the builder API
while keeping its current device/command/sub-editor registration methods as
adapters during migration.

### 3.3 Registration

```rust
// Point owner.
extension.register_extension_point(TRACK_PANEL);

// Contributor.
extension.register_extension::<MyPanel>(TRACK_PANEL);
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
```

`clear_own_extensions(TRACK_PANEL)` is permitted; clearing another extension's
contributions is not.

### 3.4 Context and handles

```rust
impl ExtensionContext {
    pub fn get_extensions<T: ?Sized>(
        &self,
        point: ExtensionPoint<T>,
    ) -> Vec<ExtensionHandle<T>>;

    pub fn get<T: ?Sized>(
        &self,
        extension: ExtensionId,
    ) -> Result<ExtensionHandle<T>, ExtensionError>;
}
```

`get_extensions` returns an ordered snapshot of owned handles. No registry
reference or lock survives the query. `get` is explicit service lookup and does
not infer a point from the trait.

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
   ids are unique, owners are enabled, and ordering constraints are acyclic.

A missing point is reported after resolution, not at first registration.

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

### 4.3 Threading

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

- keep one `#[notwig_extension]` entrypoint per binary;
- add a guest export returning bounded registration descriptors for points,
  implementations, commands, and existing contributions;
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
#[extension_api(id = "com.notwig.api.panel", version = 1)]
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
#[extension_api(id = "com.notwig.api.menu", version = 1)]
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
- `context.get_extensions(POINT)` discovers unknown contributors.

Not every extension capability should become a point.

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
stable `(extension_id, implementation_id)` tie-breakers. Cycles are a resolution
error.

## 9. Integration order

Start with the smallest thing that proves the model across two genuinely
independent WASM instances.

### Milestone 0 — Proof of concept

- Add `#[extension_api]` for one trait:
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
- Keep existing device/command/sub-editor exports behind adapters.
- Add registry/invoke ABI exports and the UI-thread runtime.

### Milestone 2 — Existing host points

- Map `notwig.command`, `notwig.audio_editor.sub_editor`, and device panels onto
  the new registry.
- Introduce `NotwigCommands` as the app/UI facade.
- Keep backward-compatible author entrypoints during the migration.

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
- Registration order is irrelevant.
- Contributors cannot remove another extension's registrations.
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
