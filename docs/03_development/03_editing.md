# Editing operations

Status: design contract. It is implemented for the arranger, the audio clip
editor, and the MIDI editor. The automation adapter is the remaining editor to
wire up.

Related: [`Editing Testing`](04_editing_testing.md).

## 1. Problem

The arranger and the audio clip editor already implement duplicate, copy, paste, cut, delete, move, and trim. The automation editor will need the same operations later. A previous implementation gave each editor its own selection type, clipboard, clamping rules, destination clearing, and duplicate target math. The audio clip editor had a second clipboard and a second copy of duplicate.

The result was not only duplicated code. A fixed bug had to be fixed four times, and a test only proved that one editor behaved correctly.

This design puts the operations behind one seam. Each editor projects its content into a small model, calls the same algebra, and lowers the result into document commands.

## 2. Vocabulary

- **Editor**: a view that edits one document region. The arranger, audio clip editor, MIDI editor, and automation editor.
- **Lane**: an ordered, editable stream of content. In the arranger a lane is a track. In the audio clip editor it is the clip. In the MIDI editor it is a pitch row. In the automation editor it is one parameter.
- **Object**: a discrete editable thing with an identity. A clip, slice, note, or automation point.
- **Audio event**: the UI term for an audio slice. The domain type keeps the name `AudioSlice`; the clip editor calls it an event.
- **Span**: the half-open interval `[start, end)` with `start <= end`. A span with `start == end` is empty. Two spans intersect when `a.start < b.end && b.start < a.end`.
- **Point**: an object with a single time and no length. Automation points are points; clips, slices, and notes are spans. A point at `t` intersects `[a, b)` exactly when `a <= t < b`. The `span` field of a point is the marker `[t, t]`, not an interval.
- **Container**: an optional outer extent that owns lane time. The audio clip editor's container is the clip range. The MIDI editor has no hard container: its content length is advisory and notes may lie outside the clip range. The arranger has no container.
- **Extract**: owned clipboard content produced by copy. It holds cloned content, not document references.
- **Plan**: an ordered list of `EditOp` values produced by an operation. A plan is pure and has no IDs for newly created objects.
- **Lowering**: the adapter-side step that turns a plan into document commands, allocates IDs, and resolves created objects back into a concrete selection.

## 3. Layers and seams

```
pointer gesture
      |
      v
selection + operation          UI state, pure
      |
      v
edit algebra  ------------->   Extract (clipboard)
      |  notwig-core, pure
      v
plan + next selection
      |
      v
lowering  ----------------->   CommandPayload[]
      |  notwig-app
      v
ProjectDoc
```

There are two seams:

1. `EditModel` between a document region and the algebra.
2. `lower` between a plan and commands.

The algebra never mutates a document, performs IO, allocates IDs, or reads UI state. `EditModel` never mutates. Lowering is the only place that knows command shapes, identity allocation, and embedded payloads such as audio slices.

This is the deep module: one small algebra covers every editor, and the editor-specific detail stays behind `EditModel` and `lower`.

## 4. The edit model

```rust
pub trait EditKey: Copy + Ord + std::hash::Hash + std::fmt::Debug + 'static {}

pub enum EditorKind {
    Arranger,
    AudioClip,
    Midi,
    Automation,
}

pub struct LaneBounds {
    pub start: TimeMus,
    pub end: Option<TimeMus>,
}

pub struct ObjectView<L, O> {
    pub id: O,
    pub lane: L,
    pub span: RangeMus,
    pub kind: ObjectKind,
    pub extent: ObjectExtent,
}

pub enum ObjectExtent {
    Span,
    Point,
}

pub struct TrimLimits {
    pub min_start: TimeMus,
    pub max_start: TimeMus,
    pub min_end: TimeMus,
    pub max_end: Option<TimeMus>,
}

pub trait EditModel {
    type Lane: EditKey;
    type Object: EditKey;
    type Content: Clone + PartialEq + std::fmt::Debug;

    fn editor(&self) -> EditorKind;
    fn container(&self) -> Option<RangeMus> {
        None
    }

    fn container_trim_limits(&self) -> Option<TrimLimits> {
        let container = self.container()?;
        let eps = epsilon();
        Some(TrimLimits {
            min_start: container.start,
            max_start: container.end - eps,
            min_end: container.start + eps,
            max_end: Some(container.end),
        })
    }
    fn lanes(&self) -> Vec<Self::Lane>;
    fn lane_bounds(&self, lane: Self::Lane) -> LaneBounds;
    fn objects(&self, lane: Self::Lane) -> Vec<ObjectView<Self::Lane, Self::Object>>;
    fn object(&self, object: Self::Object) -> Option<ObjectView<Self::Lane, Self::Object>>;
    fn trim_limits(&self, object: Self::Object) -> TrimLimits;
    fn extract(&self, object: Self::Object, window: RangeMus) -> Result<Self::Content, EditError>;
    fn accepts(&self, lane: Self::Lane, kind: ObjectKind) -> bool;
}
```

`ObjectKind` is an opaque adapter tag. The algebra copies it but never inspects it. Arranger adapters use it for MIDI and audio clips. The clip editor uses it for slices. The MIDI editor uses it for notes. The automation editor uses it for point kinds.

Contract of each method:

- `container`: the editable extent, if the editor has one. Returns `None` for the arranger.
- `lanes`: every editable lane. Lanes that cannot host content are omitted. The order is the display order.
- `lane_bounds`: the hard time bounds of a lane. `end = None` means it is unbounded to the right. Arranger lanes start at zero. The audio clip editor starts its lane at `-clip_start` so events can sit before the clip. Container bounds are manual and never move during event lowering.
- `objects`: every object on the lane, ordered by `(span.start, id)`. Span objects on a lane do not overlap.
- `object`: a lookup by identity. Stale IDs return `None` and are dropped during materialization.
- `trim_limits`: absolute limits for trimming the object's start and end. The adapter derives them from the content and the minimum length. `max_end = None` means no upper limit.
- `extract`: content clipped to `window`, anchored so that the returned content starts at `window.start`. `window` lies inside the object's span. For a point, `window` is `[t, t]`. Extracting audio rewrites source sample windows; extracting a MIDI note returns the part of the note in the window. Extraction never stretches content. A failed extraction is an error, not a silent drop, so a broken selection cannot half-apply.
- `accepts`: whether a lane can host a pasted object of this kind. This is what stops a MIDI clip from landing on an audio track.

The interface stays small on purpose. Everything about clip payloads, sample windows, note splitting, and point ordering stays in the adapters. An adapter may back `objects(lane)` with an interval index; the algebra only requires the ordered result.

## 5. Selection

### 5.1 State

```rust
pub struct Selection<L, O> {
    pub objects: BTreeSet<O>,
    pub time: Option<TimeSelection<L>>,
}

pub struct TimeSelection<L> {
    pub span: RangeMus,
    pub lanes: BTreeSet<L>,
}

impl<L, O> Selection<L, O> {
    pub fn mode(&self) -> Option<SelectionMode> {
        if !self.objects.is_empty() {
            Some(SelectionMode::Objects)
        } else if self.time.is_some() {
            Some(SelectionMode::Time)
        } else {
            None
        }
    }
}
```

Both forms can be present at once. The mode decides which one an edit uses:

```
Objects if objects is not empty
else Time if a time selection exists
else None
```

The same rule drives keyboard edits, toolbar edits, and context menu edits. There is no editor-specific dispatch.

The key types are per editor. The arranger instantiates `Selection<TrackId, ClipId>`, the audio clip editor `Selection<ClipLane, SliceId>`, the MIDI editor `Selection<Pitch, NoteId>`, and the automation editor `Selection<ParamLane, PointId>`. The shell routes commands to the focused editor, which uses its own instantiation.

### 5.2 Transitions

```
select_time(span, lanes):
    require span is non-empty and lanes is non-empty
    objects = {}
    time = Some({span, lanes})

select_object(object, additive):
    if additive:
        objects ^= {object}
        time unchanged
    else:
        objects = {object}
        time = None

clear:
    objects = {}
    time = None
```

Creating a time selection always clears object selection. Non-additive object selection always clears the time selection. Additive object selection keeps the time selection. This matches the current arranger rule and gives the clip editor the same behaviour.

Stale object IDs are dropped when the selection is materialized, not when the document changes. This keeps undo simple.

### 5.3 Object selection span

Object mode needs one span for offsets, duplicate, and paste:

```
span.start = min(object.span.start)
span.end   = max over objects of (
                 object.span.end              if the object is a span
                 object.span.start + epsilon  if the object is a point
             )
epsilon    = 1 microbeat = 1/1_000_000 beat
```

The epsilon term only matters when the last object is a point. It ensures that duplicate places points after the source instead of on top of it.

### 5.4 Time selection bounds

A time selection is valid only inside the intersection of its lanes' bounds:

```
lo = max(lane.start for every selected lane)
hi = min(lane.end for every selected lane with a bound)
```

If `hi < lo`, the selection is invalid. The UI must not create it. Operations also validate it.

## 6. Materialization

Every operation starts by turning a selection into a uniform view:

```rust
pub struct Materialized<L, O, C> {
    pub mode: SelectionMode,
    pub span: RangeMus,
    pub lanes: BTreeSet<L>,
    pub members: Vec<ObjectView<L, O>>,
    pub fragments: Vec<Fragment<L, C>>,
}

pub struct Fragment<L, C> {
    pub lane: L,
    pub offset: TimeMus,
    pub length: TimeMus,
    pub kind: ObjectKind,
    pub content: C,
}

pub enum SelectionMode {
    Objects,
    Time,
}
```

Rules:

- **Objects mode**
  - `span` is the bounding span from 5.3.
  - `lanes` is the set of lanes of the selected objects.
  - `members` is the selected objects, sorted by `(lane, span.start, id)`.
  - `fragments` contains one fragment per member. `offset = member.span.start - span.start`. `length = member.span.length()`. `content = model.extract(member, member.span)`. The whole object is copied.
- **Time mode**
  - `span` is the selection span.
  - `lanes` is the selected lanes.
  - `members` every object on a selected lane that intersects the span, sorted by `(lane, span.start, id)`.
  - `fragments` contains one fragment per member whose intersection with the span is non-empty. `window = member.span ∩ span`. `offset = window.start - span.start`. `length = window.length()`. `content = model.extract(member, window)`. The object is clipped.
  - A point is included when `span.start <= t < span.end`. Its fragment has `length = 0` and `offset = t - span.start`.

Materialization is where the two selection forms become one. Everything after this point is mode-parametric, not editor-parametric.

The deliberate asymmetry is in the data, not in the operations:

- Object mode copies whole objects.
- Time mode copies only the part of each object inside the span, and keeps the gaps between them.

### 6.1 Plans

```rust
pub struct Plan<L, O, C> {
    pub ops: Vec<EditOp<L, O, C>>,
    pub next: NextSelection<L, O>,
}

pub enum EditOp<L, O, C> {
    SetContainer { span: RangeMus },
    Erase { lane: L, span: RangeMus },
    Remove { object: O },
    Split { object: O, at: TimeMus },
    Move { object: O, lane: L, span: RangeMus },
    Trim { object: O, span: RangeMus },
    Place { lane: L, span: RangeMus, kind: ObjectKind, content: C },
}

pub enum NextSelection<L, O> {
    Unchanged,
    None,
    Objects(Vec<ObjectRef<O>>),
    Time { span: RangeMus, lanes: Vec<L> },
}

pub enum ObjectRef<O> {
    Existing(O),
    Created(usize),   // index into the ordered Place ops
}
```

`Created` keeps the algebra free of ID allocation. Lowering allocates an ID for each `Place` in order and resolves the index when it builds the concrete selection.

## 7. Clipboard

```rust
pub struct Extract<L, C> {
    pub domain: EditorKind,
    pub mode: SelectionMode,
    pub length: TimeMus,
    pub lanes: Vec<L>,
    pub fragments: Vec<Extracted<L, C>>,
}

pub struct Extracted<L, C> {
    pub lane: L,
    pub offset: TimeMus,
    pub length: TimeMus,
    pub kind: ObjectKind,
    pub content: C,
}
```

- An extract owns cloned content. It never borrows the document, so a later edit or undo cannot invalidate it.
- `domain` is the editor family that produced it. Paste requires `extract.domain == model.editor()`. Pasting notes into the arranger or clips into the audio clip editor is rejected with a clear error. Cross-domain conversion, such as turning notes into a new MIDI clip, is a separate feature.
- The arranger and audio clip editor share the workspace clipboard. The MIDI
  editor keeps its own note clipboard in view state because pitch lanes and
  note content are not interchangeable with arranger/audio lanes. Within each
  clipboard, copy replaces it. Cut replaces it only after the delete has been
  lowered and applied successfully. Duplicate never touches it.
- The workspace stores a `Clipboard` enum with `Arranger`, `AudioClip`, and a
  `Devices` marker. The marker keeps the latest copy kind visible to shortcut
  routing; the actual device snapshots live in the workspace device clipboard.
  The MIDI editor stores `Extract<Pitch, NoteEntity>`. The extract's `domain`
  tag rejects a mismatch before lowering.
- The device clipboard is project-global and survives track/lane selection
  changes. Copying or cutting clips replaces the clip clipboard, and copying or
  cutting devices replaces the `Devices` marker, so paste follows the most
  recent copy.
- Paste also validates every lane with `model.accepts(lane, kind)` before any command is applied.

## 8. Operations

The operation matrix is the contract. Read it before reading the individual sections.

| Operation | Object selection | Time selection |
| --- | --- | --- |
| Copy | Copy whole objects, offsets from the bounding start. | Copy objects clipped to the span, offsets from the span start, gaps kept. |
| Cut | Copy, then remove the whole objects. | Copy clipped, then erase the span on every selected lane. |
| Paste | Place whole objects at `at + offset`; erase each destination span first. | Place clipped fragments at `at + offset`; erase the whole destination span first, so gaps come back as gaps. |
| Duplicate | Copy, paste at the bounding end, select the copies. | Copy, paste at the span end, select the new span. |
| Delete | Remove the whole objects. | Erase the span on every selected lane; objects crossing the boundary are trimmed or split. A zero-width span splits crossing objects without removing content. |
| Move | Shift the whole objects as one group; selection follows. | Clip every intersecting object to the span, shift the fragments, and leave the outside parts in place. Selection span follows. |
| Trim start/end | Resize the object edge under the pointer; selection becomes that object. | Resize the selection span; content does not change. |

Time selection move clips. The selected part of every intersecting object is cut and pasted at the shifted span. The parts that stick out of the source span stay where they are. This is Bitwig's current behaviour.

An operation means the same thing in both modes at the caller's level. It uses the same command, the same dispatch, the same clear-before-place rule, the same undo grouping, and the same selection update rules. Only three things vary by mode: where the span comes from, whether whole objects or fragments are addressed, and whether an operation removes objects or subtracts time. Those three are properties of the materialized selection, so the operation code stays shared.

The algebra exports these functions:

```rust
copy(model, selection) -> Result<Extract, EditError>
split(model, selection) -> Result<Plan, EditError>
delete(model, selection) -> Result<Plan, EditError>
cut(model, selection) -> Result<(Extract, Plan), EditError>
paste(model, extract, at) -> Result<Plan, EditError>
duplicate(model, selection) -> Result<Plan, EditError>
move_selection(model, selection, delta) -> Result<Plan, EditError>
trim_selection(model, selection, edge, to) -> Result<Plan, EditError>
trim_object(model, object, edge, to) -> Result<Plan, EditError>
trim_container(model, edge, to) -> Result<Plan, EditError>
```

### 8.1 Copy

```
view = materialize(selection)
extract = Extract {
    domain: model.editor(),
    mode: view.mode,
    length: view.span.length(),
    lanes: view.lanes,
    fragments: view.fragments,
}
```

Copy does not mutate the document and cannot fail on a valid selection.

### 8.2 Cut

```
extract = copy(selection)
plan = delete(selection)
```

The caller applies the plan and only then replaces the clipboard. If lowering fails, the old clipboard stays.

### 8.3 Paste

Inputs: an extract, a resolved target time `at`, and the target model.

```
validate every fragment's lane exists and accepts its kind
validate at >= max(lane.start for every target lane)
plan = []

if extract.mode == Time:
    plan.push(Erase { lane, span: [at, at + extract.length) } for every lane)
else:
    for each fragment:
        plan.push(Erase { lane, span: [at + offset, at + offset + length) })

for each fragment:
    plan.push(Place {
        lane: fragment.lane,
        span: [at + offset, at + offset + length),
        kind: fragment.kind,
        content: fragment.content,
    })

next selection:
    Time   -> Time { span: [at, at + extract.length), lanes: extract.lanes }
    Objects -> Objects(Created for every Place op, in order)
```

The target `at` is chosen by the gesture layer, not by the algebra:

- Time mode: `at = selection.span.start` when a time selection exists, else the cursor.
- Object mode: `at = the insertion cursor`.
- The value is snapped before it reaches the algebra. Duplicate targets are not snapped.

Paste clears before it places. Lowering must apply every `Erase` before any `Place`; otherwise a later erase would remove freshly pasted content.

### 8.4 Duplicate

```
view = materialize(selection)
extract = copy(view)
plan = paste(extract, view.span.end)
```

Duplicate never changes the clipboard. It is copy plus paste, so it inherits destination clearing, offset preservation, and selection behaviour for free.

Object mode places the copy immediately after the bounding end, so repeated duplicates tile with no gap. Time mode places the copy immediately after the selection and selects an equal-length span.

### 8.5 Delete

```
view = materialize(selection)

Objects mode:
    ops = view.members.map(|object| Remove { object })
    next = Objects(empty)

Time mode:
    ops = view.lanes.map(|lane| Erase { lane, span: view.span })
    next = Time { span: view.span, lanes: view.lanes }   // kept
```

Keeping the time selection after delete makes repeated edits predictable. Object selection is cleared because its objects no longer exist.

`Erase` is content-aware. It removes, trims, or splits objects in the span, and it is described in section 9.

### 8.5.1 Split

```
split(model, selection):
    require a zero-width time selection with at least one lane
    at = selection.span.start
    ops = Split { object, at } for every span object on a selected lane
          with object.span.start < at < object.span.end
    next = Time { span: selection.span, lanes: selection.lanes }
```

Split does not remove content. Lowering turns each `Split` into the editor's native split command: the arranger splits a clip and any crossing MIDI note, the MIDI editor splits a note into a left half with the original ID and a right half with a new ID, and the audio editor splits a slice the same way. Delete on a zero-width time selection routes here; copy, cut, and duplicate still reject an empty time selection.

### 8.6 Move

Object-mode drags can change lanes. The `Move` op carries the target lane, and `relocate_objects` is the entry point for drags that already resolved their final spans. It validates the lane and the object kind, then emits `Move` ops. Same-lane group moves use `move_selection`.

```
view = materialize(selection)
delta = clamp_move_delta(view, requested_delta)
if delta == 0: no-op

Objects mode:
    ops = view.members.map(|member| Move {
        object: member.id,
        lane: member.lane,
        span: member.span + delta,
    })
    next = Objects(view.members)

Time mode:
    source = view.span
    dest   = source + delta
    ops = Erase { lane, span: source } for every lane
        + Erase { lane, span: dest } for every lane
        + Place { lane, span: dest.start + fragment.offset, ... }
          for every fragment
    next = Time { span: dest, lanes: view.lanes }
```

Object mode moves whole objects, keeps their identities, and may change lanes. Time mode cuts the clipped fragments, clears the destination, and places them at the shifted span. Moved fragments receive new IDs. The outside parts of partially covered objects keep their identities and stay in place.

Clamping is group-wise:

```
Objects mode:
    lower = max(lane.start - member.span.start) over members
    upper = min(lane.end - member.span.end) over bounded lanes

Time mode:
    lower = lo - view.span.start
    upper = hi - view.span.end

if lower > upper: delta = 0
else:             delta = clamp(requested, lower, upper)
```

`lo` and `hi` are the time selection lane bounds from section 5.4. `hi` is `None` when every selected lane is unbounded, so the upper clamp is `None` too.

For points, `span.start == span.end == t`, so the object formula works. In time mode a point inside the span is cut and placed at the destination.

Object mode uses the lift/place rule in section 9.3. Time mode uses erase-then-place, in that order. Erasing the source first makes a destination that overlaps the source safe: the second erase only removes stationary content.

### 8.7 Trim

Trim has three targets. The pointer decides which one is active.

```
on press:
    if a time selection exists and the pointer is on its start or end handle:
        TrimSelectionEdge
    else if the pointer is on an object edge:
        clear the time selection
        select only that object
        TrimObject
    else if the pointer is on the container edge:
        TrimContainer
    else:
        other gesture
```

The time selection handle wins when both handles overlap. The object edge is checked before the object body.

#### 8.7.1 Time selection edges

```
limits = selection bounds from 5.4
epsilon = 1 microbeat

Start edge:
    a' = clamp(to, lo, selection.span.end - epsilon)
    span' = [a', selection.span.end)

End edge:
    b' = clamp(to, selection.span.start + epsilon, hi or to)
    span' = [selection.span.start, b')
```

The plan is empty. The next selection is the resized time selection. Content is untouched. This is the operation the user reaches for when they drag the in/out points of a time selection.

#### 8.7.2 Object edges

```
limits = model.trim_limits(object)

Start edge:
    s' = clamp(to, limits.min_start, limits.max_start)
    span' = [s', object.span.end)

End edge:
    e' = clamp(to, limits.min_end, limits.max_end or to)
    span' = [object.span.start, e')

ops = [Trim { object, span: span' }]
next = Objects({object})
```

The opposite edge never moves. The adapter supplies content-aware limits. For audio, a start trim advances the source window and a start extension pulls earlier source material back in. For MIDI, a start trim advances `content_start`. Section 11 lists the per-kind rules.

A point object has no edges to trim. `trim_object` returns an error for `ObjectExtent::Point`; the router never sends one.

Object trim is single-object by design. The routing above clears the time selection and selects the object that owns the grabbed edge. Multi-object trim needs a separate gesture and is deferred.

#### 8.7.3 Container edges

Only editors with a container support this. The audio clip editor's clip start and end handles are the current case.

```
container = model.container().expect("editor has a container")
epsilon = 1 microbeat

Start edge:
    s' = clamp(to, container.start, container.end - epsilon)
End edge:
    e' = clamp(to, container.start + epsilon, container.end)
```

The plan is `SetContainer { span: [s', e') }`. The span is in editor-local time. Lowering treats the difference from the old container as the trim amount, rewrites the content window so slice and note anchors stay attached to the timeline, moves the clip's arranger start when the start edge is trimmed, and renormalizes the surviving local time to start at zero. The slice and note anchors stay put on the timeline across that renormalization. The container cannot be moved or selected. This operation is orthogonal to object and time selection.

## 9. Destination clearing, splitting, and overlap

### 9.1 Erase

`Erase { lane, span }` removes every object, or part of an object, in `span`. It keeps content outside the span and never stretches content.

```
for each span object on the lane that intersects span:
    fully inside        -> remove
    covered on the left -> trim the object's end to span.start
    covered on the right-> trim the object's start to span.end
    span inside object  -> split at span.start and span.end,
                           remove the middle, keep the two ends
for each point on the lane with span.start <= t < span.end:
    remove
```

Splitting preserves content anchors. Trimming an audio slice or clip adjusts source sample windows by the sample delta of the trimmed edge. MIDI notes are split at the boundary and the right part keeps the same absolute note content.

### 9.2 Placement clears first

Every insertion emits `Erase` before `Place`. This covers paste, duplicate, and any future insert. Move and Trim are not insertions; they use the lift and place rule in 9.3. Time-mode paste erases the whole destination span, so empty gaps in the source become empty gaps at the destination. Object-mode paste erases only each fragment's destination span, so content in the gaps between objects survives.

The lowered sequence is:

```
1. SetContainer, if the plan contains an explicit container trim
2. every Erase
3. every Remove
4. every Move and Trim
5. every Place
```

The order is part of the contract. Lowering may batch or reorder commands internally only if the observable result matches this order.

### 9.3 Lift and place

Object-mode `Move` and extending `Trim` are atomic relocations. Lowering implements them with three steps:

```
1. Lift the affected objects out of the lane. Keep their content and identity.
2. Erase the destination spans on the destination lanes,
   excluding the lifted objects themselves.
3. Reinsert the lifted objects at their new spans with their original identities
   and untouched content.
```

This makes the moved set authoritative. Stationary content that overlaps a destination is trimmed or split; the moved content is never trimmed. Source gaps stay as gaps.

### 9.4 Container bounds are manual

The clip start and end move only when the user drags the container handles in the audio editor. Event and note edits never grow or shrink the clip range. Paste, duplicate, move, and trim can place audio events and MIDI notes before the clip start or after the clip end; they stay visible and editable.

Playback still follows the clip range. Events and notes outside it are drawn, selected, copied, and edited, but they do not play. The clip and MIDI editors draw the grid and ruler across the visible range, including outside the clip, and the scroll range includes the outermost objects.

The waveform is drawn only where the source has samples. Over-trimmed events leave the unsupported tail empty instead of stretching the available waveform.

### 9.5 Determinism

Members are sorted by `(lane, span.start, id)`. Ops are emitted in the phase order above, and within a phase by `(lane, span.start, id)`. The same model and selection always produce an identical plan. This is required for golden tests and for undo stability.

## 10. Lowering

Lowering is the only impure step before command application. It has four jobs.

1. Allocate IDs for every `Place`. New IDs are caller-generated, as elsewhere in the domain. Place ops are numbered; `NextSelection::Objects` refers to `Created(place_index)`, and lowering resolves those to the IDs it allocated.
2. Apply the plan in the phase order from 9.2.
3. Translate content semantics. Trimming an audio clip adjusts `source_start_sample` and the slice windows, not just the range.
4. Emit commands inside one transaction.

Command mapping:

| EditOp | Arranger | Audio clip editor | MIDI editor |
| --- | --- | --- | --- |
| `Erase` | trim, split, or delete clips | rewrite the slice list | trim, split, or delete notes |
| `Place` | `CreateClip` | grow the clip, then `SetClipSlices` | create or set notes |
| `Move` | `MoveClip` | move a slice | move a note |
| `Trim` | `EditClip` | trim a slice | set note start and length |
| `Remove` | `DeleteClip` | remove a slice | `DeleteNote` |
| `Split` | `SplitClip` | split a slice | split a note |
| `SetContainer` | not applicable | `EditClip` range and content | edit MIDI content length |

Lowering must be atomic:

- All commands for one operation belong to one undo group. The app snapshots the document once per operation, not once per command.
- If any command fails validation, no command is applied and the clipboard is unchanged.
- Embedded payloads such as audio slices are collected in memory and written once. A partially applied slice list is never observable.

## 11. Trim semantics per object kind

The algebra only knows spans and limits. The adapter and lowering own these rules.

| Kind | Start trim | End trim | Extension |
| --- | --- | --- | --- |
| MIDI clip | Advance `range.start` and `content_start` equally. Notes stay in place. | Adjust `range.end`. Content length is unchanged. | Left is limited by `content_start >= 0`. Right opens an empty tail. |
| Audio clip, single source | Advance `range.start` and `source_start_sample` by the same sample delta. | Adjust `range.end` and `source_end_sample`. | Limited by available source. No silent tail. |
| Audio clip, slices | Advance `range.start` and rewrite every slice window. Fully covered slices are dropped. | Adjust `range.end` and trim slice windows at the new edge. | The container is manual. Events may extend outside it. No stretch. |
| Audio slice | Advance `start` and `source_start_sample`, keep the source end fixed. | Adjust `source_end_sample`, keep `start` fixed. | Limited by source. The waveform draws only where samples exist. No stretch. |
| MIDI note | Advance `start`, shorten `length`. | Adjust `length`, keep `start` fixed. | Minimum length is one tick or epsilon. The container is manual. |
| Automation point | Not trimmable. | Not trimmable. | Points move and copy only. |

No operation stretches audio. Waveform scale follows the source sample position, not the trimmed span.

## 12. Snapping and gesture math

Snapping is not part of the algebra. The algebra receives resolved times (`to`, `at`, `delta`) and only clamps them to model limits. This keeps the algebra free of display state and makes it deterministic.

The gesture layer owns the shared `SnapSettings` value plus the pure helpers:

```
pointer_delta(pointer, press_origin, pixels_per_beat) -> TimeMus
snapped_start(absolute_anchor, delta, step, bypass) -> TimeMus
snapped_shift(absolute_anchor, delta, step, bypass) -> TimeMus
snap(time, step, bypass) -> TimeMus
resolve_target(pointer, viewport, step, bypass) -> TimeMus
```

`SnapSettings` is the single source of truth for snapping. It holds whether
snap is enabled, whether the grid is adaptive, and the manual `SnapLevel`
(`2/1` through `1/65536`). Every editor reads the same value from `UiState`, so
the scrollbar toggle applies to the arranger, the audio editor, and the MIDI
editor at once.

Snapping is parameterised by the exact bar length (`Meta::bar_length`, derived
from both the numerator and the denominator), not by a numerator-only beat
count. `SnapSettings::step` computes the one effective step used to paint the
grid, snap gestures, and label the grid control. It does not have a separate
`display_step`. Adaptive mode chooses the finest note value or bar multiple that
leaves at least 40 px between lines and that either divides the bar or is an
integer multiple of it, so generated lines always agree with bar boundaries.
Manual levels use the selected note value exactly. The grid renderer still
elides step lines below one pixel of spacing to keep very fine manual levels
readable; bar and beat lines remain visible.

Rules:

- Snapping is absolute. Gestures snap the absolute start time, then derive the
  shift. Moving a clip or event snaps its start, not its pointer delta.
- Arranger times are already absolute. Audio events are stored relative to the
  clip start, so the audio editor snaps `clip.range.start + local_time` and then
  subtracts `clip.range.start`. MIDI notes are stored in content time, so the
  MIDI editor uses the offset `clip.range.start - midi.content_start`.
- Deltas are always measured from the press origin, never accumulated per frame.
- `snap(snap(t)) == snap(t)`.
- A bypass modifier returns the input time unchanged.
- The algebra is called once per gesture update with the resolved value. It does not remember the previous frame.
- Disabled snapping behaves like a permanent bypass. When snap is off, the
  timeline grid is drawn dimmed.
- **Shift** is the universal bypass modifier. **Alt** remains supported for the
  clip and audio editors. Both routes go through `SnapSettings::bypassed`.

| Gesture | Bypass |
| --- | --- |
| Any gesture while snap is disabled | Global snap toggle |
| Clip move or trim | Shift or Alt |
| Group move | Shift or Alt |
| Time selection move | Shift or Alt |
| Time range drag in a lane | Shift |
| Lane click or double-click clip creation | Shift |
| Ruler play start | Shift |
| Clip editor range selection | Shift |
| Clip editor event move or trim | Shift or Alt |

### 12.1 Editor chrome

The arranger and the audio clip editor use the same beat strip component. It draws the beat grid, loop range, time selection, playhead, and the arranger play-start marker.

The playhead and play-start are global transport state. Clicking the beat strip in either editor emits `SetPlayStart` with the snapped beat. The clip editor also sets its insertion cursor from the same click. The clip editor does not draw a separate play-start marker; the arranger shows the shared play-start.

The device editor and the clip editor are mutually exclusive:

```
currently_opened: Option<OpenedEditor>
OpenedEditor = DeviceEditor | ClipEditor
```

The toolbar buttons toggle each editor against the current selection. Opening one closes the other. With no valid selection, the editor shows `Select an Arrangement Clip to edit.` or `Select an Arrangement Lane to edit.`

The clip editor is closed by dragging its top handle down. Its height remains resizable. Clip start and clip end are adjusted only by dragging their handles in the clip editor; event edits never resize the clip.

### 12.2 Line editor

The core line editor is a reusable time/value surface for automation, clip gain,
pan, and other modulatable values. The canonical curve model is
`notwig_core::Curve`; the interaction widget is
`notwig_ui::widgets::line_editor`.

The widget owns only ephemeral gesture state: the current point or segment drag
and the marquee rectangle. Coarse/fine modifier changes use the shared
`fine_drag` helper, which also drives numeric fields, knobs, faders, and track
controls. The audio editor owns the point selection and time
selection, so the usual copy, cut, paste, duplicate, delete, and select-all
actions operate on the same selection. The caller also owns the curve, value
range, time bounds, snapping grid, and commit boundary. A drag previews locally
and emits one command on release.

| Gesture | Result |
| --- | --- |
| Hover the curve | Show the add-point cursor |
| Click the line | Insert a point at the press position and start dragging it |
| Click off the curve | No point is created |
| Drag a point | Move selected points in time and value without crossing neighboring points; `Shift` switches to fine steps and bypasses horizontal snapping. Toggling Shift mid-drag preserves the current offset and adds subsequent motion at the new scale. Releasing Shift keeps both the coarse and fine offsets and starts a fresh coarse drag from that point. |
| Double-click a point | Delete that point; other points at the same time remain |
| Additive click or drag on a point | Toggle or add the point to the selection |
| Drag empty space | Marquee-select points inside the rectangle and set a time selection |
| Select a point | Extend the time selection so it includes every selected point |
| Backspace | Delete selected points |
| Copy/Cut/Paste/Duplicate | Operate on the audio editor's point selection; duplicate offsets by the time-selection length and paste starts at its start |
| Option/Alt-drag a segment | Bend the segment's curvature; diagonal resize cursor on hover |
| Cmd/Ctrl-drag a segment | Move both endpoint values up or down; vertical resize cursor on hover |

Segment curvature is stored on the left point and applies only to the segment
to its right. Positive curvature eases in (slow start), negative curvature eases
out (fast start), and zero is linear. `Curve::value_at` preserves the endpoints,
holds the nearest endpoint outside the point range, and stays monotonic for
curvature in `-2..=2`. Negative curvature is the exact horizontal and vertical
mirror of the same positive magnitude. Points may share a time: the incoming
segment approaches
the first point at that time, and the exact time evaluates to the last point,
forming a vertical step. Waveform rendering applies the same curve locally when
the caller supplies a `WaveformGain`, so the gain editor, arranger, and audio
editor display the scaled result.

## 13. Invariants

1. Every span satisfies `start <= end`. A dispatchable time selection also satisfies `start < end` and has at least one lane.
2. Every object ID in a selection exists in the model after materialization. Stale IDs are dropped.
3. Span objects on one lane do not overlap after any operation.
4. Copy does not mutate the document.
5. Object-mode move preserves each object's length and content window. Time-mode move preserves the span length and clips each fragment to the source span.
6. Trim keeps the opposite edge fixed and never inverts the span.
7. Trim never stretches audio and never exceeds the adapter's limits.
8. Erase never changes content outside its span.
9. Time-mode copy and delete leave content outside the source span intact. Time-mode move only clears content at its shifted destination, in addition to removing the source span.
10. Paste assigns new IDs. No pasted object aliases a source object.
11. Destination content is erased before placed content is inserted.
12. Destination content is authoritative: placing onto existing content trims the existing content, not the placed fragments. Object-mode move follows the same rule for whole objects.
13. Event edits never change the container. Only the clip start and end handles change it. Events may lie outside it.
14. Every operation is deterministic: same model and selection, same plan.
15. One user operation is one undo group. Undo restores the document and leaves selection sanitisation to the UI.
16. The clipboard holds owned content only and survives document changes.
17. Cross-editor paste is rejected unless the extract's domain matches the target editor.
18. A failed operation applies no commands and does not change the clipboard.

## 14. Migration

The algebra was built first, then each editor moved onto it one at a time. The
steps below record that path.

1. Add `crates/notwig-core/src/edit/` with `mod.rs` (selection, extract, plan, and model types), `algebra/` (materialize, delete, paste, move, trim), and `arranger.rs`. Export the types from `notwig-core`.
2. Add the tape model, property tests, and explicit snapshot tests from [`Editing Testing`](04_editing_testing.md) before any adapter exists. The pure tests are the reference.
3. Add the arranger model (`ClipId`, `TrackId`, clip payloads) and lowering in `notwig-app/src/edit_lowering.rs`.
4. Replace the clip-level editing functions in `notwig-app/src/workspace/clips.rs`: `copy_clips`, `copy_time_range`, `cut_selection`, `paste_clips`, `delete_time_range`, `duplicate_time_range`, and the duplicate helpers. The app keeps asset and notification concerns.
5. Add the audio clip editor model (`SliceId`, one clip lane) and slice lowering in `notwig-ui/src/views/audio_editor/`. Delete the old copy, paste, and delete helpers.
6. Move the shared selection state from `notwig-ui/src/state/selection.rs` and the slice editor into the core `Selection` type. Keep view state such as viewport, cursor, and drag origin in the UI.
7. Replace the helpers in `notwig-ui/src/views/arranger/edit.rs` with the algebra and the gesture helpers. `EditTarget`, `ClipboardMode`, `duplicate_plan`, `paste_target`, `clamp_node_shift`, `clamp_bar_shift`, and `duplicate_bar_range` all disappear.
8. The MIDI adapter is wired through `MidiModel`. The automation adapter remains; it should not need new algebra.

Bugs this removed from the previous implementation:

- Slice paste and slice duplicate append onto existing slices without clearing the destination, so overlap rules are not enforced.
- The arranger and the clip editor have different duplicate target math and different clipboard lifetimes.
- Delete, cut, and duplicate are written once per editor, so a fix in one does not reach the others.
- Time selection and object selection have different dispatch paths in each editor.

## 15. Deferred

- Multi-object trim gestures. The algebra can trim one object; a future gesture can call it for a sorted set.
- Cross-domain paste and conversion, including notes to a new MIDI clip.
- Clipboard persistence across app sessions.
- Automation curves with non-linear interpolation. v0 treats points as objects with a time and a value.
- Paste onto a different lane. v0 keeps every fragment on its source lane. Pasting to the focused track or with a lane offset is a separate gesture with its own compatibility rules.
- Cross-project paste. The lane and content mapping needs a project-level identity story first.
