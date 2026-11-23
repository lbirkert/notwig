# Testing edit operations

Status: test contract for [`Editing`](03_editing.md). The tape model, property
tests, lowering tests, and gesture tests are in place. The independent tape
oracle and the reusable adapter contract suite are planned. Read the design
contract first.

## 1. Test seam

Tests cross the same seam as callers: `EditModel` in, `Plan` and `Extract` out. No test reaches into editor internals, reads egui state, or depends on command shapes except in the lowering tests.

The goal is not to test each editor separately. The goal is to prove that all editors share one behaviour, then prove that each editor projects its document into the model correctly.

## 2. Test layers

| Layer | What it proves | Where it runs |
| --- | --- | --- |
| Algebra unit tests | Each operation produces the expected plan for hand-written cases. | `notwig-core`, fast, every build. |
| Law properties | Copy, cut, duplicate, move, trim, and erase obey algebraic laws on random inputs. | `notwig-core`, proptest. |
| Tape oracle | The algebra matches an independent implementation of the same semantics. | `notwig-core`, proptest. |
| Adapter contract | Every editor behaves the same through the model seam. | Reusable suite, one fixture per editor. |
| Lowering and undo | A plan becomes commands, applies atomically, and undoes as one step. | `notwig-app`, real `ProjectDoc`. |
| Gesture math | Pointer resolution, hit routing, snapping, and clamping are correct and do not ratchet. | `notwig-ui`, pure functions. |
| Fuzzing | No panics and no invariant breaks on large random inputs. | Nightly or on demand. |
| Benchmarks | Plans stay linear in selection size. | Before release. |

The implemented layers are the gate. The tape oracle and adapter contract
suite become gate tests when they are added. Fuzzing and benchmarks catch slow
drift.

## 3. The tape model

Use a small tape model as the reference implementation and as the default fixture for algebra tests. It is not shipped. It turns edit semantics into cells so the expected result is obvious.

```rust
type Lane = &'static str;
type Tick = i64;          // one tick is 1/16 beat
type Token = u32;

#[derive(Clone, PartialEq, Debug)]
pub struct Tape {
    pub lanes: BTreeMap<Lane, BTreeMap<Tick, Cell>>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Cell {
    pub token: Token,          // identifies the source run
    pub source_offset: Tick,   // offset inside that run
}
```

An object is a run of contiguous ticks on one lane with the same token and contiguous source offsets. Splitting a run creates two runs with the same token and different starting offsets. A point is one cell.

The tape adapter implements `EditModel`:

- `objects(lane)` groups cells into runs.
- `extract(object, window)` returns `Ok(TapeContent { token, source_offset })` for the window and an error for an invalid window.
- `trim_limits` returns the run's available source and the minimum length of one tick.
- `lane_bounds` covers the used tape.
- `container` is `None` for the default tape, or the full extent for container tests.

Times are `TimeMus` values that are multiples of `62_500` microbeats (1/16 beat), so the tape and the real algebra share the same time type.

### 3.1 The oracle

The oracle implements copy, cut, paste, duplicate, delete, move, and trim directly on `Tape`. It does not call the algebra.

- Copy group: read the cells in the selection, normalize them so the earliest cell is offset zero.
- Copy range: read the cells in `[a, b)`, subtract `a` from each offset.
- Paste: clear the destination cells, then write the copied cells with fresh tokens.
- Delete group: remove the cells of the selected runs.
- Delete range: remove cells in `[a, b)` and keep the rest.
- Move group: remove the selected runs, then write them at the shifted positions.
- Move range: cut the cells in `[a, b)`, clear the destination, then write them at the shifted positions. Cells outside `[a, b)` stay in place.
- Trim: change the run's end cell; source offsets stay monotonic.
- Duplicate: copy, then paste at the selection end.

After an operation, project both the real result and the oracle result, normalize tokens by order of first appearance, and compare the tapes.

### 3.2 Why the oracle is worth the work

The oracle is the only test that can catch a plausible but wrong rule, such as time-range move failing to leave the outside parts in place or time-range copy failing to preserve gaps. Unit tests only prove the cases you thought of. The oracle proves the implementation against a second, simpler reading of the contract. When the two disagree, one of them is a bug, and the disagreement points at the rule that needs a decision.

## 4. Operation acceptance matrix

Use this table to generate cases. Times are in beats. Lane `L1` holds `A = [2, 5)` and `B = [6, 7)`. Lane `L2` holds `C = [0, 4)`.

| Operation and selection | Expected result |
| --- | --- |
| Copy objects `{A, B}` | Extract span 5. A at offset 0 length 3. B at offset 4 length 1. |
| Copy time `[3, 7)` on L1 | Extract span 4. A clipped to offset 0 length 2. B clipped to offset 3 length 1. Offsets 2 to 3 are empty. |
| Paste object extract at 10 | Erase `[10, 13)` and `[14, 15)`. Place A at 10 and B at 14. Select the two new objects. |
| Paste time extract at 10 | Erase `[10, 14)` on lane L1. Place A at 10 and B at 13. Select time `[10, 14)`. |
| Duplicate objects `{A, B}` | Paste at 7. Erase `[7, 10)` and `[11, 12)`. Place A at 7 and B at 11. Select the copies. |
| Duplicate time `[3, 7)` | Paste at 7. Erase `[7, 11)`. Place A at 7 and B at 10. Select `[7, 11)`. |
| Delete objects `{A, B}` | Remove A and B. Select nothing. |
| Delete time `[3, 7)` | Erase `[3, 7)`. A becomes `[2, 3)`. B is removed. Time selection stays `[3, 7)`. |
| Move objects `{A, B}` by +2 | A becomes `[4, 7)`. B becomes `[8, 9)`. Select both. |
| Move time `[3, 7)` by +2 | A becomes `[2, 3)` plus `[5, 7)`. B becomes `[8, 9)`. Time selection becomes `[5, 9)`. |
| Trim time start to 4 | Selection becomes `[4, 7)`. Content does not change. Plan is empty. |
| Trim time end to 2 | Invalid because `2 < start + epsilon`. Clamp to `3 + epsilon`. |
| Trim object A start to 3 | A becomes `[3, 5)`. The source offset of the first surviving cell is 1 beat, not 0. |

Run every row in both selection modes where applicable, and in every editor fixture. A rule that only works in the arranger is not a rule.

## 5. Laws

Write these as proptest properties. A law failure is a design bug, not a fixture bug.

Let `P(x)` be the normalized tape projection of the result of `x`.

1. Copy is pure. `P(copy(s)) == P(no-op)`.
2. Cut is copy then delete. `cut(s) == (copy(s), delete(s))`.
3. Duplicate is copy then paste at the end. `P(duplicate(s)) == P(paste(copy(s), span(s).end))`.
4. Paste at the source start is content-equivalent to the source region. Compare with fresh tokens normalized.
5. Move by zero is a no-op. The plan is empty and `P` is unchanged.
6. Object-mode move preserves every moved object's length.
7. Move preserves relative offsets between moved objects.
8. Time-mode move clips every intersecting object to the span. Parts outside the span stay in place.
9. Time copy preserves gaps. The occupied offsets in the extract equal the occupied offsets in the source span, minus the span start.
10. Time delete leaves the complement. `P(delete_time(s, [a,b))) == P(before) - [a,b)`.
11. Erase is idempotent. Erasing the same span twice equals erasing it once.
12. Trim of a time selection changes no content. `P(trim_selection(s)) == P(before)`.
13. Trim of an object keeps the opposite edge fixed.
14. Trim of an object always leaves a span of at least the adapter's minimum.
15. Every resulting span is valid and inside lane bounds.
16. Span objects on a lane do not overlap after any operation.
17. Pasted IDs are fresh, distinct from the source and from each other.
18. The same model and selection produce an identical plan when called twice.
19. Cross-domain paste returns an error and applies no ops.
20. An invalid selection returns an error and applies no ops.

### 5.1 Property strategies

- 1 to 3 lanes.
- 0 to 20 objects per lane.
- Disjoint span objects, so the generated model satisfies the overlap invariant.
- A mix of span objects and points.
- Random object selections and random non-empty time selections on random lane subsets.
- Occasionally a selection whose span crosses object boundaries, with empty gaps.

### 5.2 Selection state

These are ordinary unit tests, not properties.

1. Creating a time selection clears object selection.
2. Non-additive object selection clears the time selection.
3. Additive object selection keeps an existing time selection.
4. Dispatch uses object selection when both are present.
5. Stale object IDs are dropped at materialization, not when the document changes.

## 6. Adapter contract suite

The suite is a function that takes a fixture and runs the same assertions for every editor.

```rust
pub trait EditorFixture {
    type Lane: EditKey;
    type Object: EditKey;
    type Content: Clone + PartialEq + std::fmt::Debug;
    type Model: EditModel<
        Lane = Self::Lane,
        Object = Self::Object,
        Content = Self::Content,
    >;

    fn model(&self) -> &Self::Model;
    fn selections(&self) -> Vec<Selection<Self::Lane, Self::Object>>;
    fn lower_and_apply(
        &mut self,
        plan: &Plan<Self::Lane, Self::Object, Self::Content>,
    ) -> Result<Selection<Self::Lane, Self::Object>, String>;
    fn project(&self) -> Tape;
}

pub fn assert_editor_contract<F: EditorFixture>(fixture: &F) {
    for selection in fixture.selections() {
        // 1. Copy does not change the projection.
        // 2. Cut equals copy plus delete.
        // 3. Duplicate equals paste at the selection end.
        // 4. Move by zero is a no-op.
        // 5. Time delete changes nothing outside the span.
        // 6. Trim selection changes no content.
        // 7. Every plan is deterministic.
        // 8. Every resulting span is valid and in bounds.
        // 9. No lane has overlapping span objects.
        // 10. Every created selection resolves to an existing object.
    }
}
```

Fixtures:

- `ArrangerFixture`: a real `ProjectDoc` with one audio track and one instrument track, MIDI and audio clips, a cross-track selection, and a time selection over both tracks.
- `AudioClipFixture`: a real audio clip with at least two slices, including a gap and a slice that crosses the selection boundary.
- `MidiFixture`: notes on two pitch lanes, including overlapping notes on different pitches and a note that crosses the selection boundary.
- `AutomationFixture`: points on two parameter lanes, including points exactly on the selection boundaries.

The MIDI fixture can use the real `MidiModel`. The future automation fixture may
drive a test-only document model until that editor exists. The suite must not be
weakened for it. If a rule cannot be expressed, the abstraction is missing
something.

## 7. Lowering and undo tests

These use the real `ProjectDoc` and the real command path.

1. Project, plan, lower, and apply in one transaction. Then assert `doc.validate()`.
2. Snapshot the history length before and after one user operation. Assert it grew by exactly one, even when the plan lowered to several commands.
3. Undo once. Assert the document deep-equals the pre-operation document. Redo and assert the post-operation document.
4. Selection after undo is sanitized. Deleted IDs are gone; the time selection survives if its lanes exist.
5. Destination clearing. Move a clip onto another clip and assert the stationary clip is trimmed or split, never overlapped.
6. Audio slice windows. Copy a time range that starts inside a slice and paste it. Assert `start`, `source_start_sample`, and `source_end_sample` match the expected source window. Assert no sample is duplicated or stretched.
7. Time delete across two slices. Assert the left fragment, the gap, and the right fragment keep their source offsets.
8. Events outside the container. Paste, duplicate, move, and trim an event beyond the clip start or end. Assert the clip range does not change and the event stays in the set.
9. Container trim. Drag the clip start and end handles and assert the range and content window change, existing event anchors stay put, and the minimum length is enforced.
9a. Waveform available source. Over-trim an event beyond the asset and assert the waveform rect covers only the available source. Clip the event with the viewport and assert the visible source window maps without stretching.
10. Cross-lane move. Move an audio clip to an audio track and assert success. Move it to an instrument track and assert an error with no document change.
11. Cross-domain paste. Copy slices and paste into the arranger. Assert an error and an unchanged document.
12. Failure atomicity. Inject a command validation failure. Assert no command from the plan applied, the document is unchanged, and the clipboard is unchanged.

## 8. Gesture tests

Hit routing and snapping are pure functions in the UI crate. Test them without egui frames.

```rust
enum HitTarget {
    TimeSelectionEdge(Edge),
    ObjectEdge(Edge, ObjectId),
    ObjectBody(ObjectId),
    ContainerEdge(Edge),
    Empty,
}
```

Tests:

1. Precedence. With a time selection edge and an object edge under the pointer, the time selection edge wins.
2. Object edge routing. With a time selection active, pressing an object edge clears the time selection and selects only that object.
3. Object body routing. Pressing an object body starts a move of the current object selection, not a trim.
4. Empty lane routing. Pressing an empty lane starts a time selection.
5. Snap idempotence. `snap(snap(t)) == snap(t)` for a range of steps and times.
6. Snap bypass. The bypass modifier returns the raw time.
7. Delta from origin. Drag right past the clamp, then drag back left. The result follows the pointer relative to the press origin, not the last clamped value. This catches ratcheting.
8. New gesture, new origin. Release and press again. The second gesture measures from the new press position.
9. Zoom invariant. The beat under the pointer stays fixed across a zoom step.
10. Minimum selection. Trimming a time selection below epsilon clamps instead of collapsing.

### 8.1 Editor chrome

1. Shared beat strip. A click at the same x resolves to the same snapped beat in the arranger and the clip editor.
2. Play-start sync. Clicking the clip editor beat strip emits `SetPlayStart`; the arranger play-start marker moves to that beat.
3. Playhead. Both editors draw the same playhead line. The clip editor draws no separate play-start marker.
4. Editor toggles. Opening the device editor closes the clip editor and the reverse. Clicking the active toolbar button closes it.
5. Selection messages. With no selected audio clip the clip editor shows `Select an Arrangement Clip to edit.`; with no selected track the device editor shows `Select an Arrangement Lane to edit.`
6. Drag handle. Dragging the clip editor top handle down closes it. The panel remains height-resizable.
7. Manual clip bounds. Event move, trim, paste, and duplicate never emit `EditClipRange`. Only the clip start and end handles do.
8. Layering. The grid draws below audio events and remains visible outside the clip start and end.

## 9. Fuzzing

Add a `cargo fuzz` target for the algebra.

- Generate a model and a selection from arbitrary bytes.
- Call every operation.
- Assert no panic, valid spans, deterministic plans, and no overlapping output.
- Keep the fuzz target small. It is a crash finder, not a semantic oracle.

Run a longer proptest job before release:

```
PROPTEST_CASES=4096 cargo test -p notwig-core --release edit_props
```

Check failing seeds into the repository as regression tests. Every fixed bug gets a named fixture, not just a seed. Names should describe the behaviour, for example `slice_paste_clears_destination`, `time_move_keeps_whole_objects`, and `trim_audio_does_not_stretch`.

## 10. CI

Add `proptest` as a dev-dependency of `notwig-core`. Optionally add `insta` for golden plans; if you do, review snapshots like code.

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p notwig-core edit
cargo test --workspace
```

The core edit tests must run on every change. The longer property run runs nightly or before a release.

## 11. Definition of done

The editing abstraction is done when all of these are true.

- Every operation in the matrix has a passing unit test in both selection modes.
- Every adapter passes the shared contract suite.
- The tape oracle matches for all generated cases.
- Every law has a property test with a meaningful generator.
- Lowering tests prove one undo group per operation and full rollback on failure.
- Gesture tests prove the trim routing rule and no ratcheting.
- The fuzz target has run without a crash.
- The arranger and audio clip editor no longer have their own copy, cut, paste, duplicate, delete, or trim math.
- The clip editor and arranger share the workspace clipboard and the core selection types.
- `cargo clippy` is clean and the full workspace test suite passes.
