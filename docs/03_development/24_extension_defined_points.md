# Extension-defined points

Status: proposed design. The current host only accepts the built-in
`notwig.command` and `notwig.audio_editor.sub_editor` points. This page defines
a typed, extension-owned extension-point API and the runtime metadata needed to
resolve it across extension packages.

Related: [`Extension Platform Architecture`](23_extension_platform_architecture.md),
[`Extension ABI and Hosts`](19_extension_abi_and_hosts.md), and
[`Audio Sub-editor API`](../02_extensions/07_audio_sub_editor_api.md).

## 1. Goals

- An extension can declare its own namespaced extension point without changing
  the core.
- Other extensions can contribute implementations to that point.
- Registration is type-checked at compile time: the contributed type must
  implement the interface the point expects.
- The runtime still validates point existence, interface identity, contract
  version, ownership, and payload limits. Compile-time checking cannot protect
  against a package built against a different extension version.
- A point owner can consume contributions through a bounded request/response
  interface and receive results through normal host edit/UI flows.
- The first concrete use is pan-editor commands that appear in the pan editor
  context menu when their `when` condition matches.

## 2. Compile-time point API

`notwig-ext-api` owns three public concepts:

```rust
pub trait ExtensionPointInterface: 'static {
    /// Stable interface id, for example `notwig.command@1`.
    const INTERFACE_ID: &'static str;
    type Request: serde::Serialize + serde::de::DeserializeOwned;
    type Response: serde::Serialize + serde::de::DeserializeOwned;
}

pub struct ExtensionPoint<I: ExtensionPointInterface> {
    id: &'static str,
    _marker: std::marker::PhantomData<fn() -> I>,
}

impl<I: ExtensionPointInterface> ExtensionPoint<I> {
    #[must_use]
    pub const fn new(id: &'static str) -> Self { /* ... */ }

    #[must_use]
    pub const fn id(&self) -> &'static str { self.id }

    /// Compile error unless `T: RegisterTo<I>`.
    pub fn register<T: RegisterTo<I>>(
        &self,
        extension: &mut Extension,
        contribution_id: &str,
    ) {
        T::register(extension, self.id, contribution_id);
    }
}

pub trait RegisterTo<I: ExtensionPointInterface>: Sized + 'static {
    fn register(extension: &mut Extension, point_id: &str, contribution_id: &str);
}
```

The registration API is a trait bound rather than runtime type inspection:

```rust
pub trait PointExtension<I: ExtensionPointInterface>: Sized + 'static {
    fn new() -> Self;
    fn invoke(&mut self, request: &I::Request) -> I::Response;
}

impl<I, T> RegisterTo<I> for T
where
    I: ExtensionPointInterface,
    T: PointExtension<I>,
{
    fn register(extension: &mut Extension, point_id: &str, contribution_id: &str) {
        extension.register_point_extension::<I, T>(point_id, contribution_id);
    }
}
```

A contribution that does not implement the point interface fails to compile at
`.register::<T>()`. No dynamic cast or runtime downcast is involved.

## 3. Command-compatible point interface

Command-like points reuse the existing command contract instead of inventing a
second execution model:

```rust
pub struct CommandPoint;

impl ExtensionPointInterface for CommandPoint {
    const INTERFACE_ID: &'static str = "notwig.command@1";
    type Request = CommandContext;
    type Response = CommandEffect;
}

impl<C: CommandExtension> PointExtension<CommandPoint> for C {
    fn new() -> Self { C::new() }
    fn invoke(&mut self, context: &CommandContext) -> CommandEffect {
        self.execute(context)
    }
}
```

An extension that owns an editor declares a point in its own crate:

```rust
// notwig-ext-example crate
pub const PAN_EDITOR_COMMANDS: ExtensionPoint<CommandPoint> =
    ExtensionPoint::new("notwig.ext.example.pan_editor.commands");
```

Another extension contributes a command:

```rust
use notwig_ext_api::{Extension, notwig_extension};
use notwig_ext_example::PAN_EDITOR_COMMANDS;

struct NudgePan;

impl CommandExtension for NudgePan {
    fn new() -> Self { Self }
    fn execute(&mut self, context: &CommandContext) -> CommandEffect { /* ... */ }
}

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension
        .register_extension::<NudgePan>(PAN_EDITOR_COMMANDS, "com.foo.nudge-pan");
}
```

This is the intended ergonomics: define a typed point once, then register with
ordinary Rust types.

## 4. Custom interfaces

A custom point interface is a marker type with serialized request/response
types:

```rust
pub struct PanEditorCommandInterface;

impl ExtensionPointInterface for PanEditorCommandInterface {
    const INTERFACE_ID: &'static str = "notwig.ext.example.pan_editor.command@1";
    type Request = PanEditorCommandRequest;
    type Response = PanEditorCommandResponse;
}
```

Requiring serializable request/response payloads is deliberate:

- the define/invoke path can cross WASM, native-library, and browser transports;
- the host can bound and validate payload sizes;
- a point owner in one guest can invoke a contributor in another guest without
  sharing pointers, `egui::Ui`, or Rust trait objects across an ABI;
- the interface remains type-checked inside each package at `.register::<T>()`.

For host-owned behavior such as commands, the built-in `CommandPoint`
short-circuits this serialization to the existing `CommandContext` /
`CommandEffect` execution path.

## 5. Runtime metadata and resolution

A code-level `ExtensionPoint` const is the authoring API, not the runtime
authority. Each extension exposes its point declarations and registrations to
the host through new metadata exports:

```text
notwig_extension_points()      -> postcard/JSON list of PointDeclaration
notwig_extension_registrations() -> postcard/JSON list of PointRegistration
```

`PointDeclaration` contains the point id, interface id, contract version,
owner extension id, and optional human-readable metadata. `PointRegistration`
contains the point id, contribution id, interface id, and the runtime handle
needed to invoke the contributed implementation.

The host resolves only when all of the following hold:

1. the point id is declared by exactly one enabled extension;
2. the point id is namespaced by that extension id;
3. the contribution targets a supported contract major;
4. the contribution interface id matches the point declaration;
5. the contribution id is unique inside the point;
6. the point owner is still enabled.

Failures are typed unresolved diagnostics. A contribution to a missing or
interface-mismatched point is never silently accepted.

`notwig.*` point ids remain reserved for host-defined points. Extension points
must be prefixed with their owner extension id.

## 6. Invocation

Point owners invoke contributions through a bounded request/response call. The
runtime stores type-erased adapters that still deserialize/serialize the
interface payload:

```rust
trait ErasedPointExtension: Send {
    fn interface_id(&self) -> &'static str;
    fn invoke_bytes(&mut self, request: &[u8]) -> Option<Vec<u8>>;
}
```

The owner calls a host-mediated operation:

```text
owner guest
  invoke_point(point_id, contribution_id, request_bytes)
    -> host dispatch
    -> contributor guest::invoke_bytes(request_bytes)
    -> response_bytes
```

This is synchronous and runs on the UI path only. The host applies the usual
budget: maximum request/response bytes, maximum contributions per point, and no
audio-thread calls.

For editor UI, the owner normally does not call arbitrary contributors during a
panel frame. It receives a resolved list of commands for its own points:

```rust
pub struct PointCommandDescriptor {
    pub point_id: String,
    pub contribution_id: String,
    pub title: String,
    pub category: String,
    pub when: Option<String>,
    pub enabled: bool,
}
```

The pan editor draws enabled commands in its context menu. Selecting one returns
an invocation request in its response:

```rust
pub struct PointCommandInvocation {
    pub point_id: String,
    pub contribution_id: String,
}
```

`AudioSubEditorResponse` carries these invocations; the host executes each
through the normal command path, checks the document revision, and applies the
resulting effect as one undo group.

## 7. Pan editor example

The pan extension declares:

```rust
pub const PAN_EDITOR_COMMANDS: ExtensionPoint<CommandPoint> =
    ExtensionPoint::new("notwig.ext.example.pan_editor.commands");
```

Its sub-editor context contains commands resolved for that point. The pan editor
draws a context menu from `context.point_commands`, filtering by `when` on the
host side. A click produces a `PointCommandInvocation`. The contributor's
`CommandExtension` runs in its own package and returns the normal
`CommandEffect`; it cannot mutate the pan editor's private state.

This gives extension-to-extension extensibility with the host still owning
capability checks, command dispatch, revision checks, and undo.

## 8. Safety and lifecycle

- Compile time: `RegisterTo<I>` proves the implementation type matches the
  interface expected by the point const.
- Load time: interface id, contract version, owner, namespace, and uniqueness
  are verified from runtime metadata.
- Call time: request/response payloads are bounded, typed by the interface, and
  validated before dispatch.
- A disabled or missing point owner disables its contributions and removes its
  commands from the owning editor context.
- Extension-defined points grant no new host authority. They route only
  declared interface payloads and host-owned effects.
- Point ids and interface ids are stable, versioned API. Changing an interface
  requires a new contract major and a new const, not an in-place reinterpretation.

## 9. Migration plan

1. Add `ExtensionPoint`, `ExtensionPointInterface`, `PointExtension`, and
   `RegisterTo` to `notwig-ext-api`; add compile-time tests proving invalid
   registrations fail and command registrations succeed.
2. Add point/registration metadata exports and host resolution. Keep
   `notwig.*` built-in point behavior unchanged.
3. Add `CommandPoint` and command registration to extension-defined points.
4. Add point-command descriptors and invocation requests to audio sub-editor
   contexts/responses.
5. Update the pan example to declare
   `notwig.ext.example.pan_editor.commands` and show contributed commands in
   its context menu.
6. Add a second test extension package that contributes a command to the pan
   point, proving cross-package compile-time typing and runtime dispatch.
7. Generalize invocation to custom serialized interfaces after the command path
   is proven.
