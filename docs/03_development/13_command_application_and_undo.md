# Command application and undo

Status: implemented. This page describes the command seam between the app and
`notwig-core`, plus the history policy in `notwig-app`. See
[`Editing`](03_editing.md) for the algebra that produces most commands.

Related: [`Architecture`](00_architecture.md), [`Code Structure`](01_code_structure.md).

## 1. The apply contract

`notwig_core::apply` is the document mutation entry point:

```rust
pub fn apply(doc: &ProjectDoc, command: &Command) -> Result<ApplyResult, CoreError>
```

`ApplyResult` contains a new `ProjectDoc`, a `StateDelta`, and `Vec<EngineMsg>`.
`apply` clones the input and mutates the clone, so a failed command leaves the
caller's document untouched. It performs no I/O, takes no locks, writes no
history, and never calls the engine. The app relies on that contract.

It is functional at the document boundary but not fully referentially
transparent: several helper paths allocate IDs internally (section 8), so the
same document and command can produce different IDs.

After a payload succeeds, `apply` calls `ProjectDoc::allocate_revision`
(`revision.saturating_add(1)`) and stores the result in `StateDelta::rev`.
Every successful command increments the revision, including a no-op. Core does
not read the wall clock; it only clamps `meta.modified_utc` to at least
`meta.created_utc`. `Command::id` exists but `apply` reads only
`command.payload`: there is no idempotency, deduplication, or reply correlation.

## 2. Dispatch and deltas

Dispatch is one exhaustive `match` on `CommandPayload` in `apply_payload`. Each
arm mutates the clone and records a private `Changes` value:

- `changes: Vec<DocChange>` is the ordered change list.
- `affected: AffectedSet` has BTree sets for tracks, clips, notes, devices,
  chains, and assets.

`AffectedSet` is a coarse index, not a complete description. Transport, meta,
track order, and routing changes appear only in `changes`. Some payloads emit
an upsert for a related entity without adding it to `affected`: `CreateTrack`
emits `ChainUpserted` and `TrackOrder` but adds only the track. Consumers that
need completeness must read `changes`.

`Workspace` currently ignores `StateDelta`; it uses only the new document and
engine messages. Delta and affected sets are available but unused today.

## 3. Batches and failure

`Workspace::apply` handles one payload. `Workspace::apply_batch` handles a
lowered operation: it clones `before` and a working document, applies payloads
in order to the working copy, and aborts on the first error. Only after every
command succeeds does it assign `self.doc`, record one history entry, and
publish or rebuild the engine. Earlier commands existed only in the local
clone, and engine messages collected before the failure are discarded. Single
commands share this property because `Workspace::apply` assigns only on `Ok`.

A batch is therefore atomic at the document level and one user operation is one
undo group. Errors are flattened to `String`. No external side effects are
published before the whole batch succeeds.

## 4. Engine and schedule emission

`apply` emits `EngineMsg` values but does not apply them:

| Payload family | Emitted messages |
| --- | --- |
| Transport, pause, stop | `SetTransport`; stop also emits `AllNotesOff` |
| Tempo | `UpdateSchedule` and `SetTransport` |
| Clip, note, slice edits | `UpdateSchedule` |
| Track mixer | `SetTrackState` for every track |
| `SetParam` | `SetParam` |
| Most track/device structure | none or `UpdateSchedule` |

`Workspace` extracts only `SetTransport` and `AllNotesOff` (`control_updates`).
It then republishes the complete playback schedule, track state, and parameters
from the new document. Asset updates use their own publish path. Structural
commands take a different path: their messages are ignored and
`request_engine_rebuild` starts an asynchronous build.

`CommandPayload::requires_plan_rebuild` is true for tempo, time signature, track
active/create/delete/move, routing, sends, device insert/remove/move/bypass/
state, and monitor mode. Mixer, parameter, clip, note, recording, and asset
commands are not structural.

## 5. History policy

`History` stores complete `ProjectDoc` snapshots plus a `structural` flag:

- At most 200 undo entries; recording past the cap drops the oldest.
- Recording clears the redo stack.
- `SetTransport` (including loops), `SetRecording`, `Pause`, and `Stop` are not
  recorded. Tempo and time-signature changes are recorded.
- `apply_batch` records one entry regardless of payload kinds.
- Undo/redo swap snapshots and do not allocate a new document revision.
- Undo of a structural entry rebuilds the engine; other entries republish
  control state. Selection is not restored by history.

History is in-memory only and is reset when an autosave is recovered.

## 6. Who owns what

| Owner | Owns | Does not own |
| --- | --- | --- |
| UI lowering (`edit_lowering.rs`) | Plan-to-command ordering, new IDs, created-selection resolution | Validation policy, history, engine calls |
| `Workspace` | Calling apply/apply_batch, document assignment, dirty flag, history, rebuild vs control publish, undo/redo | Command semantics, plan algebra |
| `notwig_core::apply` | Dispatch, per-command mutation and checks, delta/change construction, revision, engine messages | History, I/O, engine execution, UI state |
| `History` | Snapshot stacks, cap, structural tag, redo clearing | Applying commands, deciding rebuilds |
| Engine | Consuming control updates and prepared runtimes, rendering | Document mutation, undo |

## 7. Validation responsibilities

Validation is layered with a final document check. `ProjectDoc::validate`
checks document-wide invariants: one Master, chain/device references,
lane/clip membership, and routing. Entity validators check clips, MIDI data,
tracks, and transport inside individual apply arms, and `apply` validates the
finished document before allocating a revision. A command that would leave an
invalid document fails atomically. Missing sends/devices remain command-specific
no-ops where documented (`RemoveSend`, `RemoveDevice`). Lowering validates the
edit model before emitting commands but does not pre-validate command legality.

## 8. Known gaps

- Explicit commands take caller-supplied IDs, including create, split, duplicate,
  and note payloads. Exceptions inside `apply` allocate IDs for implicit work:
  `clear_range` creates a `ClipId` and crossing `NoteId`s, and audio range/slice
  rewriting creates `SliceId`s. These use `ids::new_uuid`, which is random UUID
  v4 under the default `rand` feature.
- Behaviour for stale or missing IDs is still inconsistent between arms even
  though the final document is validated.
- `StateDelta` and `AffectedSet` are unused by the app, and `affected` is
  incomplete.
- A batch increments the revision once per command, not once per batch.
- `Workspace` converts typed `CoreError` into `String`, losing the variant.
- `CommandPayload::Stop` in core returns to the loop start when looping,
  otherwise zero, but `Workspace::stop` (the user-facing Stop path) republishes
  the captured play start instead. The two stop semantics differ today and
  should be unified.
