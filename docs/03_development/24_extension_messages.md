# Extension messages

Status: proposed design. This page replaces the earlier typed extension-point
draft with a host-mediated, message-based extension API. The host remains the
owner of registration, dispatch, validation, commands, and UI composition.

Related: [`Extension Platform Architecture`](23_extension_platform_architecture.md),
[`Extension ABI and Hosts`](19_extension_abi_and_hosts.md), and
[`Audio Sub-editor API`](../02_extensions/07_audio_sub_editor_api.md).

## 1. Why messages

Extensions run in separate WASM memories or native library instances. They
cannot safely share Rust trait objects, `egui::Ui`, references, or pointers.
Message passing gives extension-to-extension calls one transport-neutral shape:

- registration is data;
- calls are addressed messages;
- replies are correlated messages;
- UI drawing is a bounded panel-frame reply, not a shared `Ui`;
- state updates are host-owned command effects returned from handlers;
- the host can validate, limit, order, drop, and trace every hop.

Extension points become message topics. A topic such as
`notwig.ext.example.pan_editor.context_menu.draw` is the extension-defined
contract.

## 2. Host-owned registry

The authoritative registry lives on the UI thread in a host service named
`NotwigCommands` (the name may change before implementation):

```text
NotwigCommands
  extensions: ExtensionId -> LoadedExtensionHandle
  subscriptions: TopicId -> ordered Vec<Subscription>
  commands: CommandId -> CommandRoute
  requests: RequestId -> PendingRequest
  revision: document revision at dispatch
```

There is no guest-owned global registration table that the host must trust.
Each extension exports a bounded registration descriptor when loaded:

```text
notwig_extension_registry() -> serialized ExtensionRegistryDescriptor
```

`ExtensionRegistryDescriptor` contains:

- message subscriptions, each with topic id, handler id, payload schema id,
  and optional ordering;
- command routes, each with command id, point/topic id, metadata, and `when`;
- devices, audio sub-editors, and other host-point adapters already supported.

The host stores these descriptors in `NotwigCommands` and discards the
serialized form. Guest handler tables only map handler ids to local
implementations. The host never asks the guest to enumerate live state again.
Registrations are immutable for one loaded generation; changing them reloads
the extension.

## 3. Message model

One dispatched message:

```rust
pub struct ExtensionMessage {
    pub id: MessageId,
    pub topic: TopicId,
    pub sender: ExtensionId,
    pub correlation: Option<MessageId>,
    pub payload: MessagePayload,
    /// Document revision captured by the host, when applicable.
    pub revision: Option<u64>,
    /// Host-calculated UI/focus context for this dispatch.
    pub context: MessageContext,
}
```

`MessagePayload` is a bounded byte buffer plus a schema id. Typed helpers wrap
it:

```rust
pub struct MessageTopic<T> {
    id: &'static str,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T> MessageTopic<T> {
    #[must_use]
    pub const fn new(id: &'static str) -> Self { /* ... */ }
    #[must_use]
    pub const fn id(&self) -> &'static str { self.id }
}

pub trait MessagePayload: serde::Serialize + serde::de::DeserializeOwned {}
```

A handler receives a decoded payload and a dispatch context:

```rust
pub struct MessageContext {
    pub sender: ExtensionId,
    pub topic: TopicId,
    pub revision: u64,
    pub focus: CommandFocus,
    pub active_clip: Option<ClipId>,
    pub commands: Commands,
}

pub trait MessageHandler<T>: Send + 'static {
    fn handle(&mut self, message: &T, context: &mut MessageContext) -> MessageOutcome;
}
```

`Commands` is the only route back into the host. It can:

- publish a message;
- request/reply to a topic;
- run a host command;
- request an editor/focus change;
- show a notification;
- request a repaint.

Handlers return `MessageOutcome` carrying a bounded `CommandEffect`, outgoing
messages, and optional UI requests. The host applies effects after the dispatch
finishes, checks the revision before and after the handler, and groups edits into
one undo entry.

## 4. Dispatch modes

### Publish

Fire-and-forget event delivery to every subscription on the topic:

```text
commands.publish(PAN_EDITOR_RESIZED, payload)
```

Each handler is called synchronously on the UI thread within a per-message
budget. Handler order is deterministic: topic order, extension id, handler id.
Errors and over-budget handlers are logged and drop only that handler.

### Request/reply

A request returns a `RequestId` immediately and a reply arrives as a message to
the sender with `correlation = Some(request_id)`:

```text
let request = commands.request(PAN_CONTEXT_MENU_DRAW, draw_request);
```

The host holds the pending request in `NotwigCommands`, routes it to the topic,
collects zero or more replies, and expires it at the end of the UI frame or after
a configured deadline. Request payloads, reply payloads, and reply counts are
bounded.

Request/reply is the primitive for extension-to-extension calls. It does not
block the UI thread and works identically over WASM, native, and browser
transports.

### Command

Commands remain host-owned edit functions with palette/keybinding metadata.
`commands.run(command_id, context)` is a host command invocation; the target may
be a built-in command or an extension command. The command's `CommandEffect`
returns through the same revision-checked, one-undo-batch path as today.

## 5. UI drawing messages

A message handler cannot receive a live `egui::Ui`. UI messages return one of two
bounded responses:

1. **Semantic UI descriptions.** For menus, the handler returns items with
   labels, icons, enabled state, ordering, and an action message/command. The
   owner draws the menu in its own guest context.
2. **Child panel frames.** For arbitrary drawing, the handler returns a
   validated `PanelFrame` plus a requested size. The host composites the frame at
   the rectangle named by the request. The owner never receives another
   extension's textures or `Ui`.

This is how the pan editor context menu works:

```text
pan editor guest
  request(notwig.ext.example.pan_editor.context_menu.draw, {
      rect, input, theme, clip_context
  })
    -> NotwigCommands
    -> matching command-extension handlers
    -> replies: ContextMenuItems / PanelFrame
    -> host aggregates and composites the menu
```

The pan editor does not need a direct function call. A contributor can be a
different package written by a different author. The host owns focus, z-order,
clipping, texture remapping, and input forwarding.

### Context menu reply

A command-style contributor returns menu items:

```rust
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub order: i32,
    pub command: Option<CommandId>,
    pub action: Option<MessagePayload>,
}
```

A drawing contributor returns a `PanelFrame` with a size hint. The host places
those panels in the pan editor's context menu and routes pointer/keyboard events
back through the same message protocol.

## 6. Pan editor example

The pan editor defines its topics in a shared contract crate:

```rust
// notwig-ext-example-pan-api
use notwig_ext_api::{ContextMenuItem, MessageTopic};

pub const PAN_EDITOR_CONTEXT_MENU_DRAW: MessageTopic<PanContextMenuDraw> =
    MessageTopic::new("notwig.ext.example.pan_editor.context_menu.draw");

pub const PAN_EDITOR_CONTEXT_MENU_ACTION: MessageTopic<PanContextMenuAction> =
    MessageTopic::new("notwig.ext.example.pan_editor.context_menu.action");
```

The pan editor publishes `PAN_EDITOR_CONTEXT_MENU_DRAW` when its context menu
opens. It receives menu items and child frames through host dispatch and draws
them.

Another extension subscribes in its registration entrypoint:

```rust
use notwig_ext_api::{Extension, MessageContext, MessageOutcome};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.subscribe::<NudgePanMenu>(PAN_EDITOR_CONTEXT_MENU_DRAW);
}

struct NudgePanMenu;

impl MessageHandler<PanContextMenuDraw> for NudgePanMenu {
    fn handle(
        &mut self,
        request: &PanContextMenuDraw,
        context: &mut MessageContext,
    ) -> MessageOutcome {
        if !request.has_selected_slices() {
            return MessageOutcome::none();
        }
        MessageOutcome::menu_item(ContextMenuItem {
            id: "notwig.ext.example.pan-commands.nudge-left".into(),
            label: "Nudge Pan Left".into(),
            enabled: true,
            order: 10,
            command: None,
            action: Some(/* action payload */),
        })
    }
}
```

The pan editor shows the item only when the reply is enabled. Clicking it
publishes `PAN_EDITOR_CONTEXT_MENU_ACTION`; the contributor handles that message
and returns a `CommandEffect` containing an `AudioCurves` edit. `NotwigCommands`
checks the revision and applies one undo batch.

The contributor can also be a regular command contribution:

```json
{
  "id": "notwig.ext.example.pan-commands.nudge-left",
  "point": "notwig.ext.example.pan_editor.context_menu",
  "contract": "notwig.ext.example.pan_editor.context_menu@1",
  "title": "Nudge Pan Left",
  "category": "Pan",
  "when": "focus == audio_editor && selection.slices > 0"
}
```

The manifest metadata feeds the host command registry; the message topic carries
the live request/reply. The host checks that both carry the same point/topic and
contract version.

## 7. Ordering, recursion, and budgets

- Dispatch always runs on the UI thread, never the audio thread.
- Handler order is deterministic and stable.
- Recursive message depth is capped; cycles are rejected or truncated with a
  diagnostic.
- Per-message byte limits, recipient limits, reply limits, and handler fuel
  limits apply to WASM.
- A handler that mutates canonical state must return a `CommandEffect`; direct
  document mutation is impossible.
- Effects from one dispatch batch are applied after all handlers return, with a
  revision check and one undo entry.
- Messages are ephemeral. Durable extension state still belongs in host-owned
  project data or extension settings.

## 8. Relationship to current extension points

The existing built-in points remain host APIs:

- `notwig.command` maps to the host command registry;
- `notwig.audio_editor.sub_editor` maps to the audio-editor selector;
- future `notwig.panel` and clipboard points remain host-owned.

Extension-defined points are topics. A manifest may declare the topic's
contract metadata for discovery and install-time validation, but the runtime
authority is the UI-thread `NotwigCommands` registry built from exported
registrations.

This avoids the earlier typed-point/interface ABI. The type safety moves to
typed message helpers inside each package; the host enforces the shared schema
id and byte limits at the boundary.

## 9. Migration plan

1. Add `TopicId`, `MessageId`, typed `MessageTopic<T>`, `MessagePayload`,
   `MessageContext`, `MessageOutcome`, and guest-side `Commands` helpers.
2. Add `ExtensionRegistryDescriptor` export and load it into a UI-thread
   `NotwigCommands` registry. Keep current device/command/sub-editor exports
   unchanged behind adapters.
3. Implement publish dispatch and host command invocation.
4. Implement request/reply with correlation ids and per-frame expiration.
5. Add `PAN_EDITOR_CONTEXT_MENU_DRAW` / `ACTION` contract crate and adapt the pan
   editor to request menu items.
6. Add `notwig-ext-example-pan-commands` package that subscribes to the draw
   topic and returns a menu item plus a pan-curve effect.
7. Add host composites for child panel replies and route pointer/keyboard input
   to the contributing handler.
8. Add tests: deterministic ordering, recursion limits, stale revision,
   request expiration, context-menu visibility by `when`, one undo entry, and
   cross-package WASM dispatch.

## 10. Open questions

- Should messages be broadcast to all subscribers or optionally addressed to a
  specific contribution id?
- Does the context menu owner draw semantic items itself, or does the host
  composite child frames for all contributors?
- Should request/reply complete within one UI frame by default, or is a
  multi-frame deadline acceptable for analysis-style requests?
- How much of the registry descriptor should be visible in the extension
  explorer for diagnostics?
