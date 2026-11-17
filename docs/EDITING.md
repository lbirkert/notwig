# Editing Model

This document defines the editing contracts for the arranger timeline and the audio clip editor. It is derived from the existing arranger implementation and is intended to be the source of truth for future refactors.

Related: [`FRONTEND.md`](./FRONTEND.md) for shell/focus/shortcut routing, [`ARRANGER.md`](./ARRANGER.md) for track and lane UI, [`CLIPS.md`](./CLIPS.md) for clip identity, overlap, and copy semantics, [`DEVICES.md`](./DEVICES.md) for the device rack.

## 1. Time and intervals

All musical time is `TimeMus`, a rational number of quarter-note beats (`t ∈ Q`, `t >= 0`).

Every UI conversion to `TimeMus` is rounded to microseconds of a beat:

```
TimeMus::new(round(t * 1_000_000) as i64, 1_000_000)
```

A `RangeMus` is the half-open interval:

```
[start, end), start <= end
length = end - start
empty <=> start == end
contains(t) <=> start <= t < end
intersects(u) <=> start < u.end && u.start < end
```

Samples convert to musical time as:

```
samples_per_beat = sample_rate * 60 / tempo_bpm
samples_to_mus(n) = n / samples_per_beat
mus_to_samples(t) = round(t * samples_per_beat)
```

## 2. Arranger geometry

Let:

```
W          = arranger available width
header_w   = min(230, 0.45 * W)
timeline_x = arranger_left + header_w
ROW_H      = 64
RULER_H    = 30
SCROLLBAR_H= 14
master_h   = ROW_H if a Master track exists else 0
rows_h     = max(available_height - SCROLLBAR_H - 4, 60)
scroll_h   = max(rows_h - master_h, ROW_H)
```

The Lane-Tree occupies `[arranger_left, timeline_x)`. The Timeline-Content occupies `[timeline_x, arranger_right)`.

The Master track is always rendered last in a fixed row directly above the scrollbar. It never participates in vertical scrolling or track dragging.

Track header indentation for depth `d` is:

```
indent(d) = min(14 * d, 0.35 * header_w)
header_left = row_left + indent(d)
```

## 3. Viewport and coordinate conversion

The viewport is `(start, ppb)` where:

```
start ∈ TimeMus, start >= 0
ppb ∈ [4, 800] pixels per beat
```

Coordinate conversion:

```
x(t) = timeline_x + (t - start) * ppb
t(x) = max(0, start + (x - timeline_x) / ppb)
```

For any pointer coordinate `px`, zoom must preserve the beat under the pointer:

```
b = start_old + (px - timeline_x) / ppb_old
start_new = max(0, b - (px - timeline_x) / ppb_new)
ppb_new = clamp(ppb_old * factor, 4, 800)
```

Panning changes `start` by `delta` beats and clamps it to `>= 0`.

## 4. Adaptive grid and snapping

With bar length `B = time_signature_numerator`, the candidate snap steps are:

```
1/64, 1/32, 1/16, 1/8, 1/4, 1/2, 1, B, 2B, 4B, 8B
```

The selected step is the first candidate `s` satisfying:

```
s * ppb >= 12
```

If none satisfies it, the last candidate is used.

Snapping is:

```
snap(t) = round(t / step) * step
```

When snapping is bypassed, `snap(t) = t`.

Bypass rules in the current implementation:

| Operation | Bypass modifier |
| --- | --- |
| Clip move / trim | Alt |
| Group clip move | Alt |
| Bar-selection move | Alt |
| Time-range drag in a lane | Shift |
| Lane click / double-click clip creation | Shift |
| Ruler play-start click/drag | Shift |
| Clip-editor bar selection | Shift |
| Clip-editor slice move / trim | Alt |

The grid is drawn once over Timeline-Content, behind all rows and clips. It is not drawn per lane.

Grid line strength:

- Bar boundary: strong.
- Whole beat: medium.
- Sub-beat: faint.

## 5. Selection system

All editors use the same two selection forms.

### Node selection

A node is one discrete editable object:

- Arranger: a clip (`ClipId`).
- Clip editor: an audio slice (`SliceId`).

Node selection is an additive set with one primary node.

```
N ⊆ NodeId
```

### Bar selection

A bar selection is one half-open time interval and the scope it applies to.

```
B = [a, b), a < b
```

- Arranger scope: a set of tracks.
- Clip editor scope: the clip being edited.

### Selection transitions

The two forms are mutually exclusive when a bar selection is created, but additive node selection does not erase an existing bar selection.

```
select_bar(B):
    N = ∅
    bar = B

select_node(n, additive=false):
    N = {n}
    bar = ∅

select_node(n, additive=true):
    N = N XOR {n}
    bar unchanged

clear:
    N = ∅
    bar = ∅
```

Action dispatch uses node selection first:

```
use_nodes if N != ∅
else use_bar if B exists
else nothing
```

Clicking an empty lane clears both forms. Dragging an empty lane replaces both forms with a bar selection.

### Active editing focus

The arranger and the clip editor share one shortcut router.

```
edit_focus ∈ { Timeline, ClipEditor }
```

Focus rules:

- Opening the clip editor gives it focus.
- Clicking anywhere in the arranger gives the timeline focus.
- Clicking anywhere in the clip editor gives the clip editor focus.
- Closing the clip editor returns focus to the timeline.
- Keyboard edit actions dispatch only to the focused editor.
- The clip editor may remain open while the timeline is focused.

This means timeline copy, cut, paste, duplicate, delete, and move shortcuts continue to work while the clip editor is open, as long as the timeline has focus.

### Selection markers

A bar selection renders:

- a translucent fill over the selected interval;
- a start marker at `a`;
- an end marker at `b`.

The fill and markers are drawn above all nodes and audio slices. The markers are drawn on the beat strip and through the selected lanes. Node selection is rendered by outlining each selected node.

### Bar-selection move

Given original interval `[a, b)`, pointer beat delta `Δ`, snap bypass `z`, and clip length `Lc`:

```
L = b - a
a' = clamp(snap(a + Δ, z), 0, Lc - L)
shift = a' - a
b' = a' + L
```

The shift is applied to all nodes intersecting `[a, b)`:

- Arranger: clips on the selected tracks.
- Clip editor: slices in the clip.

The same shift is applied to the bar selection itself.

If any moved node would leave `[0, Lc)`, `shift` is clamped so the whole moved set remains inside the clip.

## 6. Clip interactions

A drag starts with:

```
pointer origin p0
clip origin interval [s0, e0)
original track track0
```

Let the current pointer x-coordinate be `px`, and:

```
Δ = (px - p0.x) / ppb
```

Let `z` be the snap bypass flag.

### Move

```
s' = snap(s0 + Δ, z)
e' = s' + max(e0 - s0, 1/16)
```

The moved clip targets the track under the pointer if that track's kind is compatible with the clip kind. Otherwise it remains on `track0`.

### Trim start

```
s' = snap(s0 + Δ, z)
if s' < e0: range = [s', e0)
else: keep [s0, e0)
```

### Trim end

```
e' = snap(e0 + Δ, z)
if e' > s0: range = [s0, e')
else: keep [s0, e0)
```

Move and trim use pointer-position differences from the drag origin. They must never use per-frame pointer deltas.

Resize handles exist only on the clip title strip, at its left and right edges.

## 7. Track drag and ordering

Track drag insertion is determined by row centers:

- If the pointer is above the center of a row, the insertion point is before that row.
- Otherwise it is after the row.
- Dropping after the final row inserts at the end.
- Parent/index are inherited from the neighbouring row's position in the track forest.
- Dropping a track into itself or one of its descendants is invalid.
- Master is not draggable.
- Master must remain clickable; no drag interaction may cover its header.

## 8. Audio slice model and clip editor

The clip editor is audio-only. It is opened for a selected audio clip, replaces the device panel in the bottom editor slot, and receives `EditFocus::ClipEditor` when the user presses inside it. Clipboard, duplicate, and delete shortcuts run against the clip editor only while it holds edit focus. See [`FRONTEND.md`](./FRONTEND.md).

An audio clip has one `asset_id`. Audio clips have two content modes:

```
slices_enabled = false, slices empty  -> legacy single-source mode
slices_enabled = true                 -> slice mode, including an empty set (silence)
slices non-empty                      -> always slice mode
```

Any explicit `SetClipSlices` enters slice mode, even when the supplied set is empty.

Each slice may reference an asset:

```
slice.asset_id = None  -> use the clip's base asset
slice.asset_id = Some  -> use that project asset
```

Dropping an audio file onto the clip editor imports it as a new slice at the dropped beat, enters slice mode, extends the clip if needed, and references the imported asset explicitly.

### Clip-local time

The clip editor uses clip-local musical time:

```
l ∈ [0, Lc), Lc = clip.range.length()
```

A slice is:

```
(l_i, source_start_i, source_end_i)
```

Its displayed local interval is:

```
[l_i, l_i + d_i)
d_i = samples_to_mus(source_end_i - source_start_i)
```

There is no time-stretch. Source duration determines displayed duration.

### Clip editor viewport and beat strip

The clip editor has the same local viewport rules as the arranger:

```
pixels_per_beat ∈ [4, 800]
start >= 0
x(t) = content_left + (t - start) * pixels_per_beat
```

Zoom preserves the beat under the pointer. Middle-drag pans; middle-drag vertically zooms; horizontal scroll pans. A horizontal scrollbar moves the viewport.

The beat strip renders:

- bar and beat lines from `TimelineGrid`;
- a `CLIP START` marker at `l = 0`;
- a `CLIP END` marker at `l = Lc`;
- bar-selection `IN` and `OUT` markers;
- the insertion cursor.

Clicking or dragging the beat strip sets the insertion cursor. Clicking or dragging empty lane space with Shift bypasses snapping.

### Clip boundary handles

The `CLIP START` and `CLIP END` markers are draggable resize handles.

```
minimum clip length = 1/16 beat
Δ = (pointer.x - press_origin.x) / pixels_per_beat
```

Start handle:

```
new_start = clamp(snap(clip.start + Δ, z), 0, clip.end - 1/16)
clip.range = [new_start, clip.end)
```

End handle:

```
new_end = max(snap(clip.end + Δ, z), clip.start + 1/16)
clip.range = [clip.start, new_end)
```

Alt bypasses snapping.

Boundary edits change the clip range before slice data is adjusted. Slice local starts are rewritten so slice content remains anchored to the timeline:

```
left_trim = new_start - old_start
slice_window = [left_trim, left_trim + new_length)
```

Slices outside the new window are trimmed or removed.

### Node selection

In the clip editor, nodes are audio slices. The slice title strip selects and moves a slice. Cmd/Shift+click toggles slice selection. Non-additive slice selection clears the bar selection.

The slice body does not select the node. Dragging the body creates a bar selection, matching the arranger rule that the clip title strip owns node selection while the body owns time selection.

### Bar selection

Dragging empty lane space creates a bar selection over `[a, b)`. Dragging the bar-selection region moves the bar selection and every slice intersecting its original interval, exactly as described in section 5.

### Slice move

Slice starts must remain inside the clip:

```
ε = 1 microbeat
l_max = max(0, Lc - ε)
anchor' = clamp(snap(anchor + Δ, z), 0, l_max)
shift = anchor' - anchor
```

When several slices are selected, `shift` is applied to every selected slice. `shift` is clamped so that:

```
min(selected starts) + shift >= 0
max(selected starts) + shift <= l_max
```

Relative offsets between selected slices are preserved.

### Slice trim start

```
l_i' = clamp(snap(l_i + Δ, z), 0, l_max)
Δs = round((l_i' - l_i) * samples_per_beat)
source_start_i' = clamp(source_start_i + Δs, 0, source_end_i - 1)
```

The right edge stays fixed when `Δs` equals the musical shift.

### Slice trim end

```
Δs = round(Δ * samples_per_beat)
source_end_i' = max(source_start_i + 1, source_end_i + Δs)
```

Alt bypasses slice move and trim snapping.

### Destination clearing

Every operation that places clip content on a track clears the destination range first.

```
place(track, range, clip):
    clear_range(track, range)
    insert(clip)
```

This applies to:

- clip creation
- duplication
- paste
- move
- clip-range edits

`clear_range` removes destination content, never permits overlap, and preserves non-overlapping fragments:

```
existing fully inside range          -> remove existing
range covers existing start          -> trim existing end to range.start
range covers existing end            -> trim existing start to range.end
range lies inside existing           -> split existing at range.start
                                        then trim the right part to range.end
```

For audio slice mode, trimming and splitting use slice windows. Slice source ranges are adjusted by the sample delta of the local trim; sliced content is never stretched.

### Shared editing actions

Clip editor and arranger actions are selected by the same rule: nodes first, then bar selection.

Shortcut mapping:

```
Cmd/Ctrl+C  copy
Cmd/Ctrl+X  cut
Cmd/Ctrl+V  paste at cursor
Cmd/Ctrl+D  duplicate
Delete      delete
Drag        move
```


#### Copy

Node copy stores each selected node with an offset from the earliest selected node:

```
o_i = l_i - min(l_j)
```

Bar copy trims each intersecting node to `[a, b)` and stores offsets from `a`:

```
o_i = max(l_i, a) - a
```

For an audio slice, the trimmed source range is:

```
Δs_start = round((max(l_i, a) - l_i) * samples_per_beat)
Δs_end   = round((min(l_i + d_i, b) - l_i) * samples_per_beat)
source_start_i' = source_start_i + Δs_start
source_end_i'   = source_start_i + Δs_end
```

#### Paste

The paste target is the active bar-selection start when a bar is selected, otherwise the insertion cursor:

```
c = bar.start if bar selection exists else cursor
l_i' = c + o_i
```

Every pasted node receives a new ID. The pasted selection becomes the pasted node set for node copy, or `[c, c + (b - a))` for bar copy.

#### Duplicate

Duplicate means copy followed by paste at a deterministic target:

```
node duplicate target = max(l_i + d_i) over the selected nodes
bar duplicate target  = b
```

The duplicated nodes or bar selection become the active selection. This applies equally to timeline nodes and clip-editor slices.

#### Cut

```
cut = copy + delete
```

#### Delete

Node delete removes the selected nodes.

Bar delete removes `[a, b)` and leaves the outside parts:

```
keep [l_i, a) if l_i < a
keep [b, l_i + d_i) if l_i + d_i > b
```

If both parts survive, the left part keeps the original ID and the right part receives a new ID. Empty slice sets silence the clip; they do not restore legacy mode.

The same slice-window rules apply when deleting a time range in the arranger. An audio clip with multiple slices is split at both deletion boundaries and its slice source ranges are trimmed accordingly.

#### Move

Node drag moves the selected node set as described in section 5. Bar drag moves every intersecting node and the bar selection.

If an edit places slice content beyond the clip end, the clip range is extended first:

```
required_end = max(start_i + duration_i)
if required_end > clip_length:
    clip_end = clip_start + required_end
```

`SetClipSlices` is applied after the range extension.

## 9. Audio slice rendering

For an audio clip starting at musical time `b0`, a slice starting at `l_i` is rendered from:

```
region_start_mus = b0 + l_i
region_start_sample = mus_to_samples(region_start_mus)
source_sample(n) = source_start_i + (n - region_start_sample)
```

Rendering stops at:

```
min(region_start_sample + source_duration_samples, mus_to_samples(clip.range.end))
```

Slices are mixed additively into the track buffer.

Waveform previews map source samples into the asset's peak table by absolute position:

```
peak_index(source_sample) = floor(source_sample / asset_samples * peak_count)
```

The mapping must not normalize by the trimmed source span. Trimming or cutting audio changes the visible sample window, not the horizontal scale of the waveform. Slice waveforms use the peak table and duration of the slice's own asset.

## 10. Required invariants

1. All musical times are `>= 0`.
2. `snap(snap(t)) = snap(t)`.
3. Zoom preserves the beat under the pointer.
4. Panning never moves `start` below zero.
5. Move preserves clip length; trim preserves a non-empty interval.
6. Time selection always satisfies `start <= end`.
7. Slice source ranges are non-empty and trim never inverts them.
8. Slice `start` lies in `[0, clip_length)`.
9. Master is always visible, pinned, clickable, and never draggable.
10. Grid lines are rendered once over Timeline-Content, not per lane.
11. The device panel and clip editor are mutually exclusive bottom editors.
12. A shortcut-driven action does not also get a redundant toolbar button.
13. The clip editor is audio-only; there is no MIDI note editor in the current build.
14. Copy, duplicate, and delete route to the editor that holds edit focus.
15. Copies are independent; no clip edit propagates to another clip (see [`CLIPS.md`](./CLIPS.md)).

## 11. Required tests

Selection and editing:

- Node and bar selection are mutually exclusive when a bar is created.
- Additive node selection preserves the existing bar selection.
- Action dispatch prefers node selection over bar selection.
- Bar selection clamps to `[0, Lc)` and preserves its length while moving.
- Node move preserves relative offsets and clamps to the clip.
- Bar move moves only intersecting nodes and clamps as one group.

Clip editor actions:

- Node copy/paste preserves offsets and assigns new IDs.
- Bar copy trims audio source ranges and offsets them to the bar start.
- Paste targets the insertion cursor.
- Node duplicate places clones after the selected node set.
- Bar duplicate places clones at the bar end and selects the new range.
- Cut copies before deleting.
- Node delete removes only selected nodes.
- Bar delete splits a slice that spans the deleted range.
- Bar delete trims the left edge, trims the right edge, and removes fully covered slices.
- Deleting all slices leaves the clip in slice mode and renders silence.
- Edits beyond the clip end extend the clip range before applying slices.
- Dragging clip start/end markers emits range edits and clamps to minimum length.
- Destination clearing splits a covering clip and preserves both fragments.
- Duplicate and paste clear the destination before inserting.
- Deleting a timeline time range splits multiple audio slices and trims their sources.
- Waveform peak lookup uses absolute asset samples and does not stretch trimmed audio.

Viewport and strips:

- Zoom preserves the clip-local beat under the pointer.
- Panning never moves the viewport start below zero.
- Bar start/end markers, clip start/end markers, and the cursor use the shared grid coordinates.
