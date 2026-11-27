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

`notwig-ext-api` makes the point type the interface description and keeps the
registered implementation as the method type argument:

```rust
pub trait ExtensionPointInterface: 'static {
    /// Stable interface id, for example `notwig.command@1`.
    const INTERFACE_ID: &'static str;
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
}

/// Implemented by an interface marker for each contribution type it accepts.
pub trait Accepts<T: ?Sized>: ExtensionPointInterface {
    fn register(extension: &mut Extension, point_id: &str, contribution_id: &str);
}

/// Erases the specific interface marker while preserving the contribution type.
pub trait PointRegistration<T: ?Sized> {
    fn register(self, extension: &mut Extension, contribution_id: &str);
}

impl<I, T> PointRegistration<T> for ExtensionPoint<I>
where
    I: Accepts<T>,
{
    fn register(self, extension: &mut Extension, contribution_id: &str) {
        I::register(extension, self.id, contribution_id);
    }
}

impl Extension {
    /// Compile error unless the point accepts `T`.
    pub fn register_extension<T>(
        &mut self,
        point: impl PointRegistration<T>,
        contribution_id: &str,
    ) {
        point.register(self, contribution_id);
    }
}
```

The call site is the requested form:

```rust
extension.register_extension::<NudgePan>(PAN_EDITOR_COMMANDS, "com.foo.nudge-pan");
```

`ExtensionPoint<T>` here uses `T` as the *interface marker*. The implementation
type is the `register_extension::<T>` argument. That separation is necessary:
one point can accept many different contributor types, so a point cannot be
parameterized by a single implementation type. The bound is still fully
compile-time checked through `Accepts<T>`.

A command-compatible interface marker:

```rust
pub struct CommandPoint;

impl ExtensionPointInterface for CommandPoint {
    const INTERFACE_ID: &'static str = "notwig.command@1";
}

impl<C: CommandExtension> Accepts<C> for CommandPoint {
    fn register(extension: &mut Extension, point_id: &str, contribution_id: &str) {
        extension.register_point_command::<C>(point_id, contribution_id);
    }
}
```

An extension declares a point in its own crate:

```rust
// notwig-ext-example crate
pub const PAN_EDITOR_COMMANDS: ExtensionPoint<CommandPoint> =
    ExtensionPoint::new("notwig.ext.example.pan_editor.commands");
```

Another package contributes:

```rust
use notwig_ext_api::{CommandContext, CommandEffect, CommandExtension, Extension, notwig_extension};
use notwig_ext_example::PAN_EDITOR_COMMANDS;

struct NudgePan;

impl CommandExtension for NudgePan {
    fn new() -> Self { Self }
    fn execute(&mut self, context: &CommandContext) -> CommandEffect { /* ... */ }
}

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_extension::<NudgePan>(
        PAN_EDITOR_COMMANDS,
        "com.foo.nudge-pan",
    );
}
```

A type that does not implement `CommandExtension` fails at the
`register_extension::<T>` call. No runtime downcast is involved.

## 4. Custom interfaces and cross-extension calls

Yes, extensions can call each other. They cannot call each other's Rust trait
methods directly because packages run in separate WASM memories or native
library instances. Instead, a custom point interface is a typed, bounded RPC
contract:

```rust
pub struct PanEditorDrawInterface;

impl ExtensionPointInterface for PanEditorDrawInterface {
    const INTERFACE_ID: &'static str = "notwig.ext.example.pan_editor.draw@1";
}

impl<T: PanEditorDrawExtension> Accepts<T> for PanEditorDrawInterface {
    fn register(extension: &mut Extension, point_id: &str, contribution_id: &str) {
        extension.register_point_rpc::<Self, T>(point_id, contribution_id);
    }
}

pub trait PanEditorDrawExtension: Sized + 'static {
    fn draw(&mut self, request: &PanEditorDrawRequest) -> PanEditorDrawResponse;
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanEditorDrawRequest {
    pub width: f32,
    pub height: f32,
    pub theme: notwig_ext_api::ThemeSnapshot,
    pub input: notwig_ext_api::PanelInput,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanEditorDrawResponse {
    pub frame: notwig_ext_api::PanelFrame,
    pub commands: Vec<notwig_ext_api::PointCommandDescriptor>,
}
```

The host stores a type-erased adapter that deserializes the request, calls the
contributor implementation in its own runtime, and serializes the response. The
calling extension invokes it through a host-mediated `call_point` operation:

```text
owner guest --request bytes--> host point router --> contributor guest
owner guest <--response bytes-- host point router <-- contributor guest
```

This works for commands, analysis, metadata queries, and other bounded custom
APIs. Interface registration remains compile-time typed locally; the host checks
the interface id and contract version at load time and enforces byte limits at
call time.

For host-owned behavior such as commands, `CommandPoint` maps the same request
and response to the existing `CommandContext` / `CommandEffect` path.

### Drawing across extensions

Passing `egui::Ui`, `android::Context`, or a host pointer to another package is
not possible or safe. Drawing therefore has two supported forms:

1. **Data-first contributions.** A point contributes commands or descriptors;
   the owner draws them itself and returns host-owned effects. This is the
   preferred context-menu path.
2. **Child panel surfaces.** A drawing interface returns a validated
   `PanelFrame` for a requested rectangle and input snapshot. The owner requests
   a child surface; the host invokes the contributor and composites the returned
   frame at the owner's rectangle. The owner never gets another package's
   textures or `Ui`. This supports extension-rendered context menus and custom
   widgets while preserving the existing panel validation, focus, and input
   routing.

Direct synchronous draw calls that pass a live `Ui` or host references remain
out of scope. A `PanelFrame` request/response is the transport-neutral
equivalent.

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

The example has three crates so that a second extension can use the point
contract without linking the pan extension's ABI exports:

```text
notwig-ext-example-pan-api/       point id, marker type, command metadata
notwig-ext-example/               pan editor extension
notwig-ext-example-pan-commands/  command extension
```

The pan extension re-exports the contract for convenience, but the command
package depends on the contract crate directly.

### 7.1 Contract crate

```rust
// notwig-ext-example-pan-api
use notwig_ext_api::{CommandPoint, ExtensionPoint};

pub const PAN_EDITOR_COMMANDS: ExtensionPoint<CommandPoint> =
    ExtensionPoint::new("notwig.ext.example.pan_editor.commands");
pub const PAN_EDITOR_COMMANDS_CONTRACT: &str =
    "notwig.ext.example.pan_editor.commands@1";
```

The point id is namespaced by the pan extension package id. The contract crate
is the stable dependency shared by the point owner and contributors.

### 7.2 Panning editor extension

The pan package keeps its existing `notwig.ext.example.pan` audio sub-editor
contribution. Its entrypoint declares the point in addition to registering the
sub-editor:

```rust
use notwig_ext_api::Extension;
use notwig_ext_example_pan_api::PAN_EDITOR_COMMANDS;

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_audio_sub_editor::<PanSubEditor>(PAN_SUB_EDITOR_ID);
    extension.register_point(PAN_EDITOR_COMMANDS);
}
```

The pan editor receives commands resolved for its own points in
`AudioSubEditorContext`:

```rust
for command in &context.point_commands {
    if !command.enabled {
        continue;
    }
    if ui.button(&command.title).clicked() {
        invocations.push(PointCommandInvocation {
            point_id: command.point_id.clone(),
            contribution_id: command.contribution_id.clone(),
        });
    }
}
```

Only enabled commands appear in the context menu. `when` is evaluated by the
host against the current `CommandContext`, so the pan editor does not need to
reimplement condition parsing. Selecting an item returns an invocation in
`AudioSubEditorResponse`.

### 7.3 Command extension

The second package declares a normal command contribution whose `point` is the
pan editor point. Its manifest id and command id are namespaced by the command
extension:

```json
{
  "manifest_version": 2,
  "id": "notwig.ext.example.pan-commands",
  "capabilities": ["command"],
  "contributes": [
    {
      "id": "notwig.ext.example.pan-commands.nudge-left",
      "point": "notwig.ext.example.pan_editor.commands",
      "contract": "notwig.ext.example.pan_editor.commands@1",
      "title": "Nudge Pan Left",
      "category": "Pan",
      "when": "focus == audio_editor && selection.slices > 0"
    }
  ]
}
```

The implementation registers the same contribution id to the typed point:

```rust
use notwig_ext_api::{CommandContext, CommandEdit, CommandEffect, CommandExtension, Extension};
use notwig_ext_example_pan_api::PAN_EDITOR_COMMANDS;

const NUDGE_LEFT: &str = "notwig.ext.example.pan-commands.nudge-left";

struct NudgePanLeft;

impl CommandExtension for NudgePanLeft {
    fn new() -> Self { Self }

    fn execute(&mut self, context: &CommandContext) -> CommandEffect {
        // Return a host-owned AudioCurves edit for the active clip.
        CommandEffect::Apply { edits: vec![/* ... */], ui: Vec::new() }
    }
}

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_extension::<NudgePanLeft>(PAN_EDITOR_COMMANDS, NUDGE_LEFT);
}
```

This is the compile-time check in practice: if `NudgePanLeft` does not implement
`CommandExtension`, the `register_extension::<NudgePanLeft>` call fails to
compile.

### 7.4 Compile-fail checks

Add UI tests under `crates/notwig-ext/notwig-ext-api/tests/ui/`:

- `wrong_interface.rs`: a type that does not implement `CommandExtension`
  registers to `PAN_EDITOR_COMMANDS`.
- `wrong_point.rs`: a valid command registers to a point whose marker expects a
  different interface.
- `missing_dependency.rs`: a package tries to use a point contract it does not
  depend on.

Each must fail with a type-bound error, not a runtime downcast or manifest error.

### 7.5 Runtime flow

1. Load the pan package. Its point metadata declares
   `notwig.ext.example.pan_editor.commands` with interface `notwig.command@1`
   and its owner extension id.
2. Load the command package. Its registration metadata names the same point,
   contribution id, and interface id.
3. The host resolves the registration, validates that the point owner exists,
   the interface ids match, the contract major matches, and the contribution id
   is unique.
4. While the pan sub-editor is active, the host builds a `CommandContext` for
   the current audio-editor focus and includes resolved point-command
   descriptors in `AudioSubEditorContext`. Only commands whose `when` condition
   evaluates true are enabled.
5. The pan editor draws enabled commands in its context menu. Selecting one
   returns a `PointCommandInvocation`.
6. The host executes the command through the contributing extension, checks the
   document revision, applies the returned `CommandEffect`, and records one undo
   entry.
7. If the pan owner is disabled or removed, its point and the contributed
   command become unresolved and the pan editor is no longer available.

### 7.6 Tests

- Contract crate test: the point const id and manifest contract string match.
- Compile-fail tests from 7.4.
- Host resolution tests: matching interface resolves; mismatched interface,
  missing owner, duplicate contribution, and wrong contract major stay
  unresolved with typed diagnostics.
- Headless pan-editor test: the context menu lists the command only while
  `when` is true; selecting it emits an invocation and applies exactly one undo
  batch.
- Cross-package integration test: build both WASM packages, load them, register
  the command into the pan point, and drive the pan context menu from a
  `PanelInput` snapshot.

## 8. Safety and lifecycle

- Compile time: `Accepts<T>` proves the implementation type matches the
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

1. Add `ExtensionPoint`, `ExtensionPointInterface`, `Accepts`, and
   `PointRegistration` to `notwig-ext-api`; add compile-time tests proving
   invalid registrations fail and command registrations succeed.
2. Add point/registration metadata exports, host resolution, and the
   `call_point` host import. Keep `notwig.*` built-in point behavior unchanged.
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
