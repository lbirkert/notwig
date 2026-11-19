# Code duplication and shared-abstraction audit

Scope: all workspace crates (`notwig-core`, `notwig-project`, `notwig-dsp`,
`notwig-engine`, `notwig-plugins`, `notwig-ui`, `notwig-app`), source only,
with test setup treated separately. Line numbers refer to the tree at the time
of writing and will drift; function names are the stable anchor.

This document separates three things:

1. **Exact duplication** — one rule implemented twice.
2. **Diverged duplication** — copies that already disagree, so the
   duplication is producing visible behavior differences.
3. **Missing abstraction** — no duplicate yet, but a shared boundary is
   clearly warranted (test fixtures, modal chrome, background jobs, etc.).

Findings are ordered by correctness risk, not by file count. The goal is to
collapse each rule to one owner in the lowest layer that can own it.

---

## Summary: highest-value work

| ID | Finding | Layers | Risk of left alone |
|---|---|---|---|
| C1 | Effective-solo rule implemented twice | core `domain` ↔ dsp `graph` | High: engine can disagree with mixer commands |
| C2 | Audio routing successor/cycle rules implemented three times | core `apply` ↔ dsp `graph` ↔ engine `runtime` | High: latency, order, and audio disagree |
| C3 | Musical↔sample conversion reimplemented | core `time` ↔ engine ↔ ui | High: sample-edge drift |
| C4 | Audio source-window/slice math reimplemented | core `apply` ↔ core `edit` ↔ ui clip editor | High: arranger and clip editor disagree |
| C5 | dB→gain and EQ band decoding reimplemented | core `time` ↔ dsp `stock` ↔ ui | Medium: audible/display mismatch |
| C6 | Plan DTOs mirror core types field-for-field | core `tracks` ↔ dsp `graph` | Medium: forced double edits |
| C7 | Clip mutation bookkeeping and `Changes` helpers | core `apply`/`sequencer` | Medium: lane order and change events drift |
| C8 | Duplicate `UpdateSchedule` emission per clip command | core `apply` | Medium: redundant work and memory |
| C9 | Device parameter initialization applied twice | dsp `stock` ↔ engine `runtime`/`factory` | Medium: init order and plugin behavior |
| C10 | Timeline coordinate, navigation, and trim math | ui arranger ↔ clip editor ↔ widgets | High: user-visible coordinate drift |
| C11 | Window/modal chrome and editor state transitions | ui shell/views | Medium: window behavior drifts |
| C12 | Undo/transaction commit path | app `workspace` | Medium: structural/history invariants diverge |

The rest of the document lists the full set, including lower-priority
mechanical consolidation.

---

## Part 1 — Cross-crate domain rules

These are the findings with the widest blast radius. Each rule crosses at
least two crates, so fixing one copy alone is worse than leaving both.

### C1. Effective-solo evaluation (diverged)

Copies:
- `crates/notwig-core/src/domain.rs:258` — `ProjectDoc::effective_solo`
- `crates/notwig-dsp/src/graph.rs:256` — `solo_effective`
- Consumed independently at `crates/notwig-core/src/apply.rs:221` and
  `crates/notwig-dsp/src/graph.rs:192`, then `crates/notwig-engine/src/runtime.rs:464`.

Both compute: if no track is soloed every track is audible; master and soloed
tracks are always audible; a track is audible if any ancestor or descendant is
soloed. The graph copy is used for `PlanTrack::solo_effective`; the core copy is
used when emitting `EngineMsg::SetTrackState`.

Why this matters: the same document can produce two answers for the same track
depending on whether a plan rebuild or a mixer command happened last. Any future
rule (solo-safe, group solo, implicit master solo) must be written twice today.

Suggested owner: `notwig-core::domain` (it already has `tracks` and
`track_order`). Compute `any_solo` once and pass it, or add:

```rust
pub struct SoloContext { any_solo: bool }
impl SoloContext {
    pub fn new(doc: &ProjectDoc) -> Self;
    pub fn is_effective(&self, doc: &ProjectDoc, id: TrackId) -> bool;
}
```

Delete `graph::solo_effective` and call the core helper from `compile`.

### C2. Routing successor and cycle detection (diverged)

Copies:
- `crates/notwig-core/src/apply.rs:730` — `validate_routing`
- `crates/notwig-dsp/src/graph.rs:297` — `path_latency` destination rule
- `crates/notwig-engine/src/runtime.rs:559` (`route_track`) and
  `crates/notwig-engine/src/runtime.rs:753` (`processing_order`)

All three resolve the next node from `io.audio_out` / `parent` / master:

```rust
TrackOutput::Track(t) => Some(t),
TrackOutput::Default => parent.or((id != master).then_some(master)),
TrackOutput::None => None,
```

The copies have already diverged:
- `apply::validate_routing` treats `Default` with no parent as terminal.
- `graph::path_latency` treats it as master.
- `ProjectDoc::validate` never calls `validate_routing`, so documents loaded
  from disk are not cycle-checked at all.
- `compile` computes execution order from the track tree, while the runtime
  recomputes a topological order that also includes sends. The stored order
  and the used order are not the same.

Suggested owner: `notwig-core::tracks` for the successor rule, `notwig-dsp`
for the execution order (it owns `CompiledPlan`). Concretely:

```rust
impl ProjectDoc {
    pub fn audio_successor(&self, id: TrackId, master: TrackId) -> Option<TrackId>;
}
impl PlanTrack {
    pub fn destination(&self, master: TrackId) -> Option<TrackId>;
}
```

Then have `validate_routing` and `path_latency` call one of these, call
`validate_routing` from `ProjectDoc::validate`, and make `compile` produce the
execution order (including sends) that `ActivePlan` consumes, deleting
`runtime::processing_order`. Decide the `Default`-without-parent behavior once
and cover it with tests.

### C3. Musical-time ↔ sample conversion

Copies / bypasses:
- Canonical: `crates/notwig-core/src/time.rs:339` `mus_to_samples`, `:349`
  `samples_to_mus`, `:354` `samples_delta`
- `crates/notwig-engine/src/engine.rs:418` — private `samples_to_mus` with its
  own arithmetic and a zero-tempo guard
- `crates/notwig-engine/src/runtime.rs:222`, `:284`, `:362` — the same
  `TempoMap` literal constructed three times
- UI raw construction `TimeMus::new((beats * 1_000_000.0)...)` appears ~34
  times in `notwig-ui`/`notwig-app`; e.g. `views/render.rs:53`, `:64`,
  `views/arranger/clip.rs:226`, `:335`, `:343` (some truncate instead of
  rounding), `widgets/timeline/grid.rs:34`, `state.rs:255`.
- Ad-hoc clamping after conversion: `apply.rs:937`, `:974`, `:1031`, `:1155`.

Why this matters: rounding and clamping policy is chosen per call site. Trim,
split, extract, and UI snapping can disagree by a sample or a micro-beat, and
`extract_content` mixes `samples_delta` for the start with `mus_to_samples` for
the length.

Suggested owner: `notwig-core::time`.

```rust
impl TimeMus {
    pub fn from_beats(beats: f64) -> Self;          // round
    pub fn from_beats_clamped(beats: f64) -> Self;  // max(0)
}
impl TempoMap {
    pub fn samples_delta_u64(self, delta: TimeMus) -> u64;
}
impl ActivePlan {
    fn tempo_map(&self) -> TempoMap; // one constructor
}
```

Make every caller use these. Move the zero-tempo guard into
`TempoMap::samples_to_mus` if it matters. Add a property test that
`samples_delta_u64(t) == samples_delta(t) as u64` for `t >= 0` and a
round-trip tolerance test.

### C4. Audio source-window and slice math (cross-layer, diverged)

Copies:
- `crates/notwig-core/src/apply.rs:797` — `audio_slice_window`
- `crates/notwig-core/src/apply.rs:836` — `audio_slice_window_set` (`pub(crate)`)
- `crates/notwig-core/src/apply.rs:937` (trim start), `:974` (trim end),
  `:1031` (edit range), `:1228`/`:1238` (split)
- `crates/notwig-core/src/edit/arranger.rs:5` imports `audio_slice_window_set`,
  `:45` `trim_limits_for`, `:113` `extract_content`
- `crates/notwig-ui/src/views/audio_editor/model.rs:22` `source_duration`,
  `:30` `slice_interval`, `:38` `slice_segment`, `:115` `span`, `:193` `extract`,
  `:304` `trim_slice`
- `crates/notwig-ui/src/views/audio_editor/geometry.rs:24`, `:118`
- `crates/notwig-ui/src/widgets/timeline/clip.rs:304` (duration inline)

Each path maps a musical window onto a source-sample window with its own
bounds check. The copies disagree: `edit_clip` rejects crossing the source
start; `trim_clip_start` saturates at zero; `trim_limits_for` derives limits
from the asset duration while the slice path clamps against the slice's own
source range. The pure edit model also imports the command layer's helper,
which inverts the intended layering.

Suggested owner: `notwig-core::sequencer` (methods on `AudioClipData` /
`AudioSlice`).

```rust
impl AudioSlice {
    pub fn duration_mus(&self, map: TempoMap) -> TimeMus;
    pub fn span(&self, map: TempoMap) -> RangeMus;
    pub fn window(&self, map: TempoMap, from: TimeMus, to: TimeMus) -> Option<AudioSlice>;
}
impl AudioClipData {
    pub fn source_window(&self, map: TempoMap, offset: TimeMus, length: TimeMus)
        -> Result<Range<u64>, CoreError>;
    pub fn trim_limits(&self, map: TempoMap, asset_duration: Option<u64>) -> TrimLimits;
}
```

Then `apply` command arms, `ArrangerModel`, and the clip-editor model call the
same methods. Remove the `apply` → `edit` import. This is the change that most
needs golden tests: trim start/end, edit range, split, copy/extract for sliced
and unsliced clips.

### C5. dB→gain and EQ band schema (core ↔ dsp ↔ UI)

dB→gain:
- Canonical `Db::to_gain` (`time.rs:274`, f32)
- Inline f64 copies: `stock.rs:320` (Tool input), `:322` (Tool output), `:699`
  (EQ output), `:792` (Chain wet), `:794` (Chain output)

EQ band schema:
- Spec/defaults: `stock.rs:116` region (`eq_spec`)
- Processor decode: `stock.rs:655` `EqProcessor::update_band`, default freq
  `1_000.0`
- Display decode: `stock.rs:417` `eq_magnitude_response`, enabled decoded as
  `ParamValue::Bool(true)`
- UI decode: `views/info/device.rs:93`, `views/device_editor/eq.rs:141`, `:157`
- UI label mapping: `views/info/device.rs:251` and
  `views/device_editor/eq.rs:634` are identical

The band-enabled default already disagrees (spec default is disabled; some UI
paths treat missing metadata as enabled, others as disabled), and the missing
frequency fallback is 1 kHz in two places but 30/200/2k/8k in the spec.

Suggested owner: `notwig-dsp::stock` (the DSP owns the audible contract; UI
should not re-encode it).

```rust
impl Db {
    pub fn to_gain_f64(self) -> f64;
}
pub struct EqBandParams { enabled, kind, freq, gain_db, q }
pub fn eq_band_params(values: &BTreeMap<ParamId, ParamValue>, band: usize) -> EqBandParams;
impl EqBandType {
    pub fn from_index(u32) -> Self;
    pub fn label(self) -> &'static str;
}
```

`eq_spec`, `update_band`, `eq_magnitude_response`, `info/device.rs`, and
`eq.rs` all use it. Add a test that the processor and display decode agree.

### C6. `PlanSend` / `PlanTrack` / `PlanDevice` mirror core types

Copies:
- `crates/notwig-core/src/tracks.rs:62` — `Send`
- `crates/notwig-dsp/src/graph.rs:31` — `PlanSend` (same five fields)
- `crates/notwig-dsp/src/graph.rs:43`, `:56` — `PlanDevice`, `PlanTrack`

`PlanSend` is a field-for-field clone with a manual mapping at `graph.rs:173`.
`PlanTrack` mirrors most of `Track` and adds `solo_effective`, latency.
`PlanDevice` mirrors `DeviceInstance` and adds `spec`/`resolved`/latency.
`notwig-dsp` already depends on `notwig-core`.

All field-list copies create a maintenance hazard: every new core field forces
an edit in the plan DTO and its constructor. Suggested: reuse `Send` directly in
`PlanTrack`; for the others, add `From<&T>` / constructor helpers so the field
mapping lives next to the type instead of being open-coded in `compile`.

### C7. Clip mutation bookkeeping and `Changes` helpers

Copies (representative):
- Clip upsert + change events: `apply.rs:351`, `:372`, `:461`, `:792`, `:1098`
  (and more), each doing `affected.clips.insert` + `DocChange::ClipUpserted`
- Lane push: `apply.rs:778`, `:1116`, `:1257`, `:1320`; retain at `:902`,
  `:1106`; sort at `:882`, `:1263`
- `Changes` (`apply.rs:32`) only has `track()` and `device()` helpers; `track`
  takes an unused `_doc`. Chains are inserted into `affected.chains`
  inconsistently (`:1469` yes, device removal `:1505` no).

The five side effects per clip write are already inconsistent: `split` and
`clear_range` sort the lane, `move` and `duplicate` do not; `create_clip` can
push an unsorted id after a sorted `clear_range`. Chain-change events are
partially emitted.

Suggested:

```rust
impl Changes {
    fn clip_upserted(&mut self, id: ClipId);
    fn clip_removed(&mut self, id: ClipId);
    fn chain_upserted(&mut self, id: DeviceChainId);
}
impl ArrangerState {
    pub fn put_clip(&mut self, clip: ClipEntity);
    pub fn remove_clip(&mut self, id: ClipId) -> Option<ClipEntity>;
}
```

One owner for lane membership and one for change events.

### C8. Every clip command emits two `UpdateSchedule` messages

`apply` unconditionally appends `EngineMsg::UpdateSchedule` after
`apply_payload` (`apply.rs:54`), while every clip arm also returns
`vec![schedule_msg(doc)]` (`:281`, `:303`, `:311`, `:320`, `:338`, `:342`,
`:353`, `:374`, `:381`, `:385`, `:389`, `:393`, `:397`, `:401`).

Result: clip commands build and clone the whole schedule twice; non-clip
commands (`SetParam`, `SetDeviceState`, `RegisterAsset`, ...) build a schedule
that nothing changed. Pick one owner: either delete the per-arm messages or the
global emission. Add a test asserting exactly one `UpdateSchedule` per mutating
command and zero for non-schedule commands.

### C9. Device parameter initialization applied twice

- Canonical defaults-then-overrides: `notwig-dsp/src/stock.rs:224`
  (`instantiate_stock_params`)
- Chain special case and factory value loop: `notwig-engine/src/runtime.rs:814`
  (`build_processor`)
- Factory paths: `notwig-engine/src/factory.rs:18`, `:81`

`build_processor` applies spec defaults and values for Chain, then applies the
values again for every factory-created processor. `PluginDeviceFactory` already
called `StockDeviceFactory`, so stock processors receive values twice; plugin
processors receive defaults only through one path. Application order and
idempotence are now an implicit contract between three functions.

Suggested owner: `notwig-dsp`: one `apply_device_params(processor, spec,
values)` function, called exactly once per processor. Make “fully initialized
processor” the factory contract. For Chain recursion, either add a small
`DeviceProcessor::add_child` seam or move chain construction into the factory
that already has `plan`.

### C10. Timeline coordinate, navigation, drag, and trim math (UI)

All verified in the tree; representative copies below.

- Beat↔pixel transform: `state.rs:247` `Viewport::beat_to_x`/`x_to_beat`
  (clamps at zero) vs `widgets/timeline/grid.rs:27` (does not clamp).
- Zoom/pan: `views/arranger/navigation.rs:7-56` duplicates
  `Viewport::zoom_at`/`pan_beats` (`state.rs:267`, `:276`) and redefines the
  clamps; `views/audio_editor/navigation.rs:10-45` is a near-copy of the
  arranger handler.
- Drag pointer extraction: the
  `press_origin().or_else(interact_pointer_pos())` and
  `interact_pointer_pos().or_else(hover_pos())` chains appear ~15 times:
  `arranger/clip.rs:107`, `:155`, `:219`, `:282`,
  `arranger/selection.rs:74`, `:87`, `:110`, `arranger/track.rs:331`,
  `audio_editor/panel.rs:150`, `:162`, `:182`, `:202`, `:416`, `:494`, `:594`,
  `device_editor/eq.rs:240`, `widgets/mod.rs:179`, `:270`.
- Trim range: `arranger/clip.rs:327` `drag_range` vs
  `audio_editor/geometry.rs:45` `edge_preview_range`. The minimum length is the
  same fraction of a beat but encoded two ways (`max(1.0 / 16.0)` in beats vs
  `TimeMus::new(1, 16)` in micro-beats), so the units can drift.
- Range-selection preview/commit: `arranger/selection.rs:84` and
  `audio_editor/panel.rs:196`/`:255` recompute the same snapped shift.
- Group-shift clamping: `arranger/edit.rs:9` vs
  `audio_editor/geometry.rs:87`.

Suggested owners: `widgets/timeline/interaction.rs` for pure math, `state.rs`
for `Viewport` as the single transform. Sketch:

```rust
pub(crate) fn drag_start_pos(ui: &egui::Ui, response: &egui::Response) -> Option<egui::Pos2>;
pub(crate) fn drag_current_pos(ui: &egui::Ui, response: &egui::Response, fallback: egui::Pos2) -> egui::Pos2;
pub(crate) fn trim_range(origin: RangeMus, edge: Edge, delta: f64, grid: TimeMus, bypass: bool, min_len: TimeMus) -> RangeMus;
pub(crate) fn timeline_navigation(ui: &egui::Ui, content: egui::Rect, content_left: f32, viewport: &mut Viewport);
```

`TimelineGrid` should be constructed from `Viewport` (or share one
`BeatTransform`) so there is one clamping policy.

### C11. Window chrome and editor state transitions (UI shell)

Window chrome repeated:
- `views/settings/mod.rs:42`, `views/render.rs:25`, `views/dsp.rs:14`,
  `views/shell.rs:914` (notifications), `:938` (help),
  `views/device_editor/browser.rs:55`

Each repeats Escape handling, `.open(&mut flag)`, centering, and a local open
flag. Drift already exists: settings/render use
`.anchor(CENTER_CENTER)`, DSP computes the center manually, help and the device
browser do not close on Escape, and settings has a capture-first rule.

Editor transitions:
- `views/shell.rs:225` `open_clip_editor`, `:231` `close_clip_editor`, `:239`
  `open_device_editor`, `:245` `close_editor`, `:251` `toggle_device_editor`,
  `:259` `toggle_clip_editor`

Each manually maintains the invariant between `currently_opened`,
`clip_editor`, and `edit_focus`. They already disagree: `toggle_clip_editor` can
set `currently_opened = Some(ClipEditor)` while `clip_editor` is `None`;
`close_clip_editor` conditionally clears, `close_editor` unconditionally.

Suggested:

```rust
// widgets/window.rs
pub(crate) struct ModalSpec { title, resizable, default_size, centred, close_on_escape }
pub(crate) fn modal_window(ctx: &Context, open: &mut bool, spec: ModalSpec, body: impl FnOnce(&mut Ui));

// shell.rs
fn set_editor(&mut self, editor: Option<OpenedEditor>, clip: Option<ClipEditorState>);
```

`open_*`, `close_*`, and `toggle_*` become thin wrappers over `set_editor`.
This matches the “shared popup-window builder” already listed as deferred in
`docs/CODE_STRUCTURE.md`.

### C12. Undo/transaction commit path (app)

- `workspace/mod.rs:298` `apply`
- `:1110` `apply_batch`
- `:387` `set_sample_rate` (manual transaction)
- `:331` `undo`, `:346` `redo` (identical except vector and label)
- `:132` `rebuild_engine`, `:241` `install_built_engine` (same preserve →
  apply transport → swap)

`apply` and `apply_batch` both clone `before`/`doc`, loop payloads, OR
`requires_plan_rebuild`, record history, then either rebuild or apply engine
messages. They already differ: `apply` excludes transport/pause/stop from
recording (`:295`), `apply_batch` records unconditionally; `set_sample_rate`
hand-rolls the transaction.

Suggested private helpers on `Workspace`:

```rust
fn commit(&mut self, before: ProjectDoc, structural: bool, engine_msgs: Vec<EngineMsg>, record: bool);
fn restore(&mut self, entry: HistoryEntry);
fn swap_engine(&mut self, engine: Engine);
```

This is covered by `tests/history.rs` and `tests/workspace.rs`; refactor behind
those tests.

---

## Part 2 — `notwig-core`

### K1. Duplicate-ID checks use two idioms

Set-based insert-then-fail:
- `domain.rs:317` devices, `tracks.rs:158` sends, `tracks.rs:270` forest IDs,
  `sequencer.rs:181` notes

`contains_key` pre-checks:
- `apply.rs:493` assets, `:556` track/chain, `:753` clip, `:1137` split,
  `:1285` duplicate, `:1413` device, `:1427` child chains

Same rule and same error construction in ten places. Add:

```rust
pub fn unique<T: Ord + Copy + Display>(seen: &mut BTreeSet<T>, id: T) -> Result<(), CoreError>;
pub fn absent<K: Ord + Display, V>(map: &BTreeMap<K, V>, id: &K) -> Result<(), CoreError>;
```

### K2. Track hierarchy rules enforced three times; hierarchy stored twice

- `apply.rs:580` `create_track`, `:692` `move_track`, `tracks.rs:134`
  `Track::validate`
- `Track.parent` vs `TrackForest.children` are two representations,
  synchronized by hand at `apply.rs:613`, `:714`, `:646`; consistency checked
  at `tracks.rs:303`.

A new parent rule or hierarchy command must update all three. Suggested:
`TrackForest::attach/detach` owns mutation and updates `Track.parent`; a
`ProjectDoc::check_parent` owns the pure eligibility rule. Add a property test
that `track_order.parent_of(id) == track.parent` after every hierarchy command.

### K3. Send validation copied and drifted

- `tracks.rs:158` validates sends as part of `Track::validate`.
- `apply.rs:234` `SetSend` reimplements the target-kind rule with the same
  error text (`apply.rs:244` vs `tracks.rs:175`) but reports a missing target
  as `NotFound`, and does not call `doc.validate()`.

Suggested: `Send::validate_in(&ProjectDoc)` and
`Track::upsert_send(Send)`, used by both paths.

### K4. Clip ordering comparator implemented three times

- `sequencer.rs:295` `sort_lane` (missing clip → `TimeMus::zero()`)
- `protocol.rs:353` `PlaybackSchedule::from_doc` (sort by start then id)
- `edit/arranger.rs:211` `ArrangerModel::objects` (same as protocol)

The fallback for a missing clip already differs, which can reorder corrupt
lanes differently in the schedule than in the view. Add
`ClipEntity::order_key() -> (TimeMus, ClipId)` and one
`sort_lane(lane, clips)`; decide one missing-clip policy.

### K5. Validation rule/constructor boilerplate

- `RangeMus::non_empty` (`time.rs:213`) exists but is dead; the same rule is
  retyped at `sequencer.rs:128` and `apply.rs:362`.
- Empty-name rule repeated at `domain.rs:29`, `tracks.rs:117`, `apply.rs:160`,
  `:564`.
- Playhead-negative rule at `domain.rs:77`, `apply.rs:84`.
- `CoreError::ValidationFailed` is constructed in dozens of places across `apply`,
  `sequencer`, `domain`, `tracks`, `time`.
- `SetClipSlices` (`apply.rs:451`) re-wraps an existing `CoreError` into
  `ValidationFailed`, changing the variant and doubling the message prefix.

Add `CoreError::validation(reason)` (and use `RangeMus::non_empty`), then
consolidate the common rules. This is mechanical but removes the biggest source
of copy-paste in core.

### K6. Edit algebra bypasses its own helpers

- `paste` (`edit/algebra.rs:257`), `clamp_time_move` (`:432`), and
  `lane_bounds_intersection` (`:622`) each implement the same lane-bounds fold;
  only the third is factored.
- Container trim defaults duplicated at `edit/mod.rs:92` and
  `algebra.rs:552`.
- `Selection::select_time` (`edit/mod.rs:184`) duplicates `Selection::time`
  (`:228`).

Have `paste` and `clamp_time_move` call `lane_bounds_intersection`; add
`TrimLimits::for_container(RangeMus)`.

### K7. Device-chain slot mutation repeated

- Detach from all chains: `apply.rs:1505`, `:1527`
- Clamp insert index: `apply.rs:1485`, `:1531`, `tracks.rs:195`

Extract `detach_device(doc, id)` and `insert_at(chain, id, index)`.

---

## Part 3 — `notwig-dsp`

### S1. Smoothing metadata is declared but unused; field lists are manual

- `params.rs:123` `SmoothingSpec`, `params.rs:156` `ParamSpec.smoothing`, but
  all constructors set `None` (`params.rs:182`, `:197`, `:212`); nothing reads
  it.
- Actual smoothing is hardcoded per field in `ToolSmoothing`
  (`stock.rs:266`–`:337`) and `ChainProcessor` (`stock.rs:731`–`:798`). Each
  smoother is enumerated in `new`, `set_sample_rate`, `snap`, and `set_param`;
  missing one leaves a smoother at 48 kHz or non-deterministic after reset.
- Hardcoded times: `stock.rs:282`, `:296`, `:651`, `:673`, `:745`, `:760`,
  `:287`, `:301`, `:605`, `:679`.

Suggested: make `ParamSpec::continuous` carry a real `SmoothingSpec`, add
`SmoothedValue::from_spec`, and replace per-field lists with a slot-indexed
`ParamSmoothers<N>` that owns `set_sample_rate`/`snap`/`set_target`/`next`.
Keep `next(slot)` allocation-free on the audio path.

### S2. Stock device param specs repeat the same gain/dry-wet definitions

- Tool input/output gain and dry/wet: `stock.rs:85`, `:93`, `:105`
- EQ output gain: `stock.rs:154`
- Chain input/output gain and dry/wet: `stock.rs:189`, `:198`, `:206`

Same ranges/units/defaults written three times. Add `input_gain_param`,
`output_gain_param(id)`, `dry_wet_param` helpers; keep IDs explicit at call
sites.

### S3. `RangeSpec` / `NormMap` duplicate range metadata and re-clamp

- `params.rs:21` `RangeSpec`, `params.rs:66` `NormMap`, `params.rs:148`
  `ParamSpec`; constructors write min/max/step/enum data twice
  (`params.rs:96` and `:111` duplicate the log bounds).
- Clamp logic repeated at `RangeSpec::clamp` (`:61`), `NormMap::to_norm`
  (`:100`), `to_physical` (`:112`), `ParamSpec::physical` (`:221`),
  `value_from_norm` (`:243`).
- `notwig-plugins/src/rack.rs:248` builds `ParamSpec` by hand with
  `NormMap::linear`, even for enums/stepped params.

Suggested: one `NormMap::from_range(RangeSpec, kind, unit)` and one log-bounds
helper; make `ParamSpec` enforce agreement in a constructor (`try_new`) rather
than exposing independent public fields.

### S4. `EngineCaps` / `RenderConfig` overlap

`devices.rs:49` and `:56` duplicate sample rate, block size, and channels;
`graph.rs:96` copies them by hand. Add `RenderConfig::from_caps(caps,
tempo_bpm)` or store `EngineCaps` inside `RenderConfig`.

### S5. `gcd_i64` / `gcd_i128`

`time.rs:108` and `:158` are the same algorithm. Make one generic over integer
width or have `gcd_i64` call `gcd_i128`. Low priority.

---

## Part 4 — `notwig-engine`

### E1. Offline render re-implements `Engine::process` chunking

- `engine.rs:267` `process` already loops in `max_block_size.max(1)` chunks.
- `engine.rs:407` `render_offline` repeats the same loop and always passes
  `frames <= block`.

Delete the outer loop; call `process(&mut output, total)` once. If a named
`process_blocks` is preferred, put it on `Engine` and use it from both.

### E2. Loop-wrap and all-notes-off logic duplicated

- `engine.rs:303` and `:349` both wrap `playhead` to `loop_start` and set
  `all_notes_off`; the conditions differ (one checks `loop_end > loop_start`,
  the other does not).
- All-notes-off handling at `:284`/`:315`; the flag is set at `:159`, `:216`,
  `:305`, `:352`, `:401`.
- There are two “notes off pending” concepts: `Engine::all_notes_off` and
  `ActivePlan::panic_pending` (`runtime.rs:72`).

Suggested: `TransportRuntime::wrap_if_needed() -> bool` and
`segment_frames(remaining)`, and fold all-notes-off into the runtime panic flag.

### E3. Engine plan/assets duplicate `ActivePlan` state; dual writes

- Engine-owned: `engine.rs:71`
- Runtime-owned plan/schedule/assets: `runtime.rs:123`, with cloned maps at
  `:131`/`:132`
- Dual writes: `SetParam` (`engine.rs:169`), `SetTrackState` (`:186`),
  `UpdateSchedule`/`PrepareAudioAsset` (`:199`)

Every plan mutation is mirrored into runtime fields. A new `EngineMsg` that
forgets the second write silently processes stale state. End state:
`ActivePlan` owns the authoritative plan/assets/schedule and exposes
`apply_plan`, `set_schedule`, `set_asset`, `set_param`, `set_track_state`.
Lower-risk intermediate: centralize each dual write in one `Engine` method and
delegate notes-off to the runtime.

### E4. Audio/MIDI clip window filtering duplicated

- `runtime.rs:365` `render_audio_clips`
- `runtime.rs:438` `collect_events`

Both walk `schedule.tracks`, filter active tracks, compute clip sample spans,
and skip non-intersecting clips; only the payload match differs. Add
`ClipEntity::sample_span(map)` / `intersects_block` and
`PlaybackSchedule::clips_intersecting(start, end, map)` in core, then filter by
payload in the engine.

### E5. Hard-coded stereo stride

`frames * 2`, `frame * 2 + 1`, and interleaved indexing appear throughout
`engine.rs:257`, `:271`, `:327`, `:341`, `:406`; `runtime.rs:34`, `:212`,
`:471`, `:581`, `:643`; `plugins/rack.rs:115`, `:164`;
`project/assets.rs:134`. Use `const STEREO_CHANNELS: usize = 2` at minimum, or
move interleaved frame access onto `AudioAssetBuffer` if multichannel is
planned. Do not do this speculatively (real-time buffer layout).

---

## Part 5 — `notwig-plugins` and `notwig-project`

### P1. VST3 and AU adapters are structural clones

- `adapters.rs:56` `load_plugin` (raw loader)
- `adapters.rs:84` `Vst3Adapter`, `adapters.rs:130` `AuAdapter`

Both `scan`/`load` differ only by scanner type and format. `load_plugin`
repeats the same scanner/load/error mapping. Drift already exists: VST3 `scan`
honors `paths`; AU `scan` ignores `_paths` (`adapters.rs:138`).

Suggested: generic `RackAdapter<S: PluginScanner>` (or shared
`scan_rack`/`load_rack` functions) plus an explicit capability flag for
“scans individual paths”. Make the AU path policy data, not a silent copy
divergence.

### P2. Format-name and descriptor mapping duplicated

- `types.rs:13` `PluginFormat::prefix`
- `adapters.rs:46` `info_from_descriptor` repeats the same match
- Type id built at `adapters.rs:21`, reversed at `:34`; round-trip maintained
  by hand

Use `PluginFormat::prefix()` in `info_from_descriptor`; add
`PluginDescriptor::type_id_for(format, uid)`. Also note `info_from_descriptor`
hard-codes `has_editor: true` and `accepts_midi: is_instrument`, which is lossy.

### P3. `PluginHost` adapter lookup duplicated

`host.rs:137` (`load`) and `host.rs:173` (`open_editor`) both search
`self.adapters` for a matching format and return `BackendUnavailable`. Add one
private `adapter_for(format)` and decide whether `open_editor` should share
`load`'s format/blacklist checks.

### P4. Plugin parameter-index map and setter duplicated

- `rack.rs:29` builds `BTreeMap<u32, usize>`; `rack.rs:90` lookup + set
- `native_editor.rs:33` builds the same map plus initial values;
  `native_editor.rs:111` lookup + set
- `ParamId`↔`u32` conversion split at `rack.rs:91`, `:227`

Add a private `ParameterIndex` in `notwig-plugins` with `collect`, `index_of`,
`set`, `value`, plus one `param_id_from_rack`/`rack_id_from_param` pair
(the current `as u32` truncation should be deliberate).

### P5. Atomic-write / JSON persistence is inconsistent

- Atomic project save: `bundle.rs:32` (temp + rename)
- Non-atomic autosave: `bundle.rs:131`
- Atomic asset write: `assets.rs:243`
- Non-atomic plugin cache: `plugins/cache.rs:37`
- Directory creation repeated in all four

Autosave is the recovery path, so a crash mid-write can leave a truncated
snapshot while a normal save is safe. Add a private `fsutil` in
`notwig-project` (`write_atomic`, `write_json_atomic`) and use it for `save`,
`autosave`, and asset finalization. For the plugin cache, either use a local
copy of the helper or accept the difference explicitly. Do not put this in
`notwig-core` (I/O-free).

### P6. `open` vs `load_latest_autosave` finalization

`bundle.rs:41` `open` and `:149` `load_latest_autosave` both deserialize, check
schema, normalize, and validate. `open` additionally handles the wrapped
manifest path and records validation issues instead of failing; autosave does
not support the wrapped shape.

Extract `parse_doc_value` and `finalize_loaded`; let `open` collect issues and
autosave fail hard, but share schema/normalize/validate ordering.

### P7. `Manifest` duplicates `ProjectDoc` fields

`manifest.rs:5` stores `schema`, `assets`, `created_utc`, `modified_utc` which
already exist in the embedded `doc`; `bundle.rs:53` only reads
`manifest.schema` and `manifest.doc`, ignoring the rest. Worse, the wrapped
load path checks top-level `schema` but not `doc.schema`, so a mismatched
payload can pass.

Either drop the duplicated fields or document them as advisory and validate
`manifest.schema == doc.schema` on load.

---

## Part 6 — `notwig-ui` editors and widgets

### U1. Info-panel row and heading styling

- Rows: `views/info/mod.rs:150`, `views/info/time.rs:15`/`:36`,
  `views/info/device.rs:152`/`:228`, `views/info/track.rs:110`/`:190`
- Headings: `views/info/common.rs:5` `section_header` and
  `widgets/form.rs:5` `section_heading`, plus inline variants at
  `info/device.rs:86`, `info/track.rs:166`,
  `device_editor/browser.rs:145`/`:162`
- Multi-selection block: `info/track.rs:93`, `info/device.rs:16`
  (count → selectable peers → set primary → separator), with the clip path
  (`info/mod.rs:103`) already missing the peer list

Add to `info/common.rs`: `info_row`, `info_row_right`, `multi_selection_summary`,
`primary_list`. Unify heading style behind one `widgets/form.rs` helper with a
style enum; make `info/common.rs::section_header` a wrapper.

### U2. Device parameter rendering duplicated

- `device_editor/mod.rs:438` `tool_body`, `:532` `chain_body`
- `device_editor/params.rs:91` `plugin_param_row`
- `info/device.rs:211` read-only variant
- Default fetch repeated at `info/device.rs:226`, `mod.rs:498`, `:558`,
  `params.rs:123`

All compute `param_values.get(...).unwrap_or(default)`, normalize, decide
bipolar from the range, and emit `SetParam` via `value_from_norm`. Add shared
`param_value`, `param_default_norm`, `param_is_bipolar`, and a `param_knob`
helper in `device_editor/params.rs`; keep per-body layout local.

### U3. Local knob duplicates `widgets::knob`

`device_editor/mod.rs:680` defines a private `knob` with the same sweep and
`value_drag` interaction as `widgets/knob.rs:10`. The local one adds
value/label text and uses different radii. Extend `widgets/knob.rs` with
`KnobStyle { radius, bipolar, value_text, label, tooltip }` and delete the
local copy; keep the plugin editor’s thin wrapper.

### U4. EQ access, labels, and hit-testing

- Band decode: `info/device.rs:93`, `device_editor/eq.rs:141`, `:157`, `:465`;
  DSP at `stock.rs:417`, `:655`
- Labels: `info/device.rs:251` and `eq.rs:634` are identical
- Hit tests: `eq.rs:179` (hover, radius 14) vs `:250`, `:303`, `:326`
  (activation, radius 13); disabled-band fallback at `:183`, `:254`, `:309`
- Enabled default: `info/device.rs:93` and `eq.rs:141` treat missing as
  disabled; `eq.rs` band controls treat missing as enabled

Resolve through `notwig_dsp::stock::eq_band_params` and
`EqBandType::label` (Part 1, C5). In `eq.rs`, add one
`enabled_band_at(points, pos)`/`any_band_at(points, pos)` with one radius
constant; if hover needs a bigger target, make that explicit.

### U5. Clip body / name strip / color drawing duplicated

- Canonical: `widgets/timeline/clip.rs:37` (kind color + muted dim), `:47`
  (body + name strip), `:85` (selection stroke), `:98` (name label)
- Copy: `views/audio_editor/panel.rs:344`
- Color mapping duplicated: `views/shell.rs:1555` `clip_color` vs
  `widgets/timeline/clip.rs:37`
- Track color resolution: `views/arranger/track.rs:60`, `:585`, `:642` and
  `views/info/track.rs:17`

Move `clip_color(kind, muted)` to `widgets/theme.rs`, and add a
`draw_clip_body(painter, rect, label, style)` primitive used by the arranger
and the clip editor. Add `theme::track_color(&Track)` for the fallback/alpha
policy.

### U6. Slice model and selection construction duplicated

- `SliceModel` literals: `audio_editor/geometry.rs:190`,
  `audio_editor/actions.rs:76`, `audio_editor/panel.rs:278`
- Edit selection: `audio_editor/state.rs:109`, `actions.rs:65`,
  `panel.rs:277`
- Local/global range: `geometry.rs:41` exists but is reimplemented at
  `panel.rs:241`/`:272` and reversed at `actions.rs:143`

Add `SliceModel::for_clip(slices, map, clip_range)`,
`ClipEditorState::edit_selection(clip_range)`, and a `global_range` inverse
next to `local_range`.

### U7. Faders, mixer toggles, and DB constants

- Horizontal volume fader: `widgets/fader.rs:7`, `:13`
- Horizontal pan fader: `info/track.rs:362`
- Vertical volume/meter fader: `info/track.rs:445`; local `DB_MIN`/`DB_MAX` at
  `:451` duplicate `widgets/fader.rs:7`
- Mixer toggles: `info/track.rs:340` `mixer_toggle` vs
  `arranger/track.rs:113` inline M/S rects

Extend `widgets/fader.rs` with orientation and a style struct
(`bipolar`, zero line, ticks, meter) and reuse it for pan and the vertical
fader; move `mixer_toggle` to a shared widget. At minimum delete the local DB
constants.

### U8. Timeline fill, tick, marker, and scrollbar drawing

- Fill: `widgets/timeline/beat_strip.rs:103` `fill_range` vs
  `widgets/timeline/selection.rs:6` `draw_time_selection_fill` (identical
  algorithm, different color)
- Ticks/grid: `widgets/timeline/grid.rs:42` `TimelineGrid::draw` vs
  `beat_strip.rs:122` `draw_beats` (same visible range, strength
  classification, and three GRID colors)
- Markers: `selection.rs:24`, `audio_editor/navigation.rs:77`,
  `beat_strip.rs:184`
- Scrollbars: `arranger/preview.rs:74`, `audio_editor/navigation.rs:47`,
  `widgets/timeline/mod.rs:19`; `Viewport::scroll_thumb`/`scroll_start`
  (`state.rs:282`, `:289`) are an unused copy of the thumb math

Add `TimelineGrid::beat_lines(rect, content_left) -> impl Iterator<Item =
BeatLine>` and `fill_beat_range(...)`, then have grid/ruler/selection consume
them. Replace the two scrollbar wrappers with one
`viewport_scrollbar(ui, rect, viewport, content_end, id)` and either delete
`Viewport::scroll_thumb` or make the widget call it.

### U9. Clipboard shortcut resolution duplicated

- `views/audio_editor/actions.rs:22`
- `views/shell.rs:341` (copy/cut/paste block)

Both implement “native menu event OR configured shortcut”, plus the same
capture/keyboard guard. Add `Shortcuts::clipboard(ctx) -> ClipboardShortcuts`
and one `keyboard_actions_allowed` helper in `commands.rs`.

---

## Part 7 — `notwig-ui` shell and `notwig-app`

### A1. Drag-preview and drop resolution duplicated

`views/shell.rs:1037` `update_drag_preview` and `:1108`
`handle_dropped_files` both resolve pointer → clip-editor drop point → track
row → snapped time. The commit path already has a preview fallback the preview
path lacks. The commit path already
has a preview fallback the preview path lacks.

Extract `pointer_drop(&self, doc, pointer) -> Option<(DropAudioTarget,
TimeMus)>`; leave cache/warning bookkeeping in the callers.

### A2. Shortcut dispatch boilerplate

`views/shell.rs:341` onward contains ~20 repetitions of
`if shortcuts.was_pressed(ctx, Command) { emit(UiIntent::X); }`, plus the time
selection tuple built three times (`:348`, `:542`) and the copy/cut/paste
bridge (`:377`). Add `ShortcutCommand::simple_intent()` for the 1:1 commands
and dispatch them in a loop; keep contextual commands (`Play`/`Stop`,
`Duplicate`, `Delete`, `Cancel`, clipboard) explicit. Preserve `was_pressed`
ordering for overlapping bindings and keep the capture/keyboard guard outside.

### A3. Help window duplicates the command table

`views/shell.rs:947` hard-codes shortcut labels/descriptions that already live
in `commands.rs:211` `COMMANDS`; `GroupTracks` is already missing from the help
list. Add a short description to `CommandSpec` and render help from
`COMMANDS` + current bindings. Collapse `KeyChord::label`/`config_string`
(`commands.rs:47`/`:66`) onto one modifier formatter, and add
`KeyChord::plain/shift/with` constructors for the repeated literals in the
command table.

### A4. Selection “primary-or-first-valid” policy

`views/shell.rs:272` `selected_audio_clip`, `:291` `selected_track`, `:482`
`DuplicateTrack` fallback; `state.rs:126`/`:207` rebuild/additive composition.
The duplicate path omits doc validation. Add
`SelectionGroup::resolved_primary()` / `first_where(...)` and one additive
composition helper on `Selection`.

### A5. Transport toggle diverges

- `views/shell.rs:594` top-bar play → `Pause` when playing
- `views/shell.rs:412` Space → `Stop` when playing
- `gui.rs:295` `SetPlayStart` assigns `play_start` identically in both branches
- `workspace/mod.rs:409` `play`/`stop`

This may be intentional product behavior, but it is currently an accident of
duplication. Either add `UiIntent::TogglePlay` and let `Workspace` own the
policy, or add one `transport_toggle_intent(playing)` helper. Confirm the
intended difference first.

### A6. Audio-settings label/lookup/defaulting

`views/settings/mod.rs:135` draft init, `:295` `driver_label`, `:311`
`device_label`, `:327` `sample_rate_label`, `:335` `devices_for`, `:345`
`sample_rates_for`; repeated lookups at `:164`, `:298`, `:341`, `:314`, `:348`;
block-size normalization at `:142`, `:206`, `gui.rs:644`, `workspace/mod.rs:115`.

`driver_label` and `device_label` are the same function over different types.
Add read-only accessors on `AudioSettingsView` in `runtime.rs`
(`driver`, `device`, `devices_for`, `sample_rates_for`, `label_for`) and a
shared `DEFAULT_BLOCK_SIZE`/`MIN_BLOCK_SIZE`. The lazy init is also unsafe when
only the driver is set.

This overlaps the `AudioSettingsDraft` already listed as deferred in
`docs/CODE_STRUCTURE.md`.

### A7. `UiIntent` → workspace error wrappers

`gui.rs:308` onward contains ~45 copies of
`if let Err(err) = self.workspace.<op>(...) { self.notify_error(err); }` and a
dozen payload wrappers. Playhead reset/sync is also repeated at `:266`, `:696`,
`:703` and `:257`, `:673`, `:711`. `ApplyAudioSettings` already returns early
on failure while most others continue.

Add `report(Result<T, String>) -> Option<T>`, `apply_payload(payload) -> bool`,
and `with_playhead_reset(...)`. Keep genuinely special intents explicit.

### A8. Error/status/notification ownership

- `gui.rs:215` `notify_error` writes `workspace.error`, `workspace.status`, and
  a notification.
- `workspace/mod.rs:189`, `:216`, `:238`, `:319`, `:356`;
  `workspace/assets.rs:27`, `:155` write `error`/`status` directly.
- Save success is reported in both `workspace/mod.rs:367` and `gui.rs:675`,
  `:776`.
- `gui.rs:126` selects from `snapshot.error` then `workspace.status`, and
  `:893` overwrites `workspace.status` from the snapshot error.

Pick one owner: recommended `Workspace` returns typed `Result`s and owns only
long-running progress; `gui.rs` owns toasts and user-facing status. Route all
writes through one helper and remove direct field writes.

### A9. Dialog plumbing and “requires bundle” policy

`gui.rs:32` `DialogAction`, `:787` `spawn_dialog`, `:802` `poll_dialog`,
`:817` `apply_dialog`; five “no path → return” copies inside `apply_dialog`;
open-project setup duplicated in `NotwigApp::new` and the open branch of
`apply_dialog`; save-as in `handle_save_as`/`apply_dialog`.

The bundle policy is inconsistent: Import Audio refuses without a bundle
(`:559`), Drop Audio opens Save-As and defers (`:566`, `:777`), and pending
drops are replayed only after Save-As, not after Save.

Add `with_path(path, f)`, `ensure_bundle_then(action)`, and one
`open_workspace(path)` used by startup and the dialog. Preserve the deferred
drop behavior deliberately.

### A10. Engine assembly duplicated between async builder and CLI render

- `engine_build.rs:23` `EngineBuildRequest::build`: compile → `EngineConfig` →
  `UpdateSchedule` → `ApplyPlan` → `PrepareAudioAsset` loop
- `main.rs:324` `render_doc`, especially `:351`–`:389`: same sequence with a
  hard-coded 512 block size and `stock_registry`

CLI render already differs from GUI render for plugins and block size. Make one
assembly function (`Workspace::engine_for_render` or an
`engine_assembly` module) and have the CLI construct it; express “stock only”
as a flag, not a divergent path.

### A11. Background-job polling repeated

`engine_build.rs:83`, `plugins.rs:66`, `gui.rs:802` all do
`Option<Receiver<T>> + try_recv → Ready/Empty/Disconnected`; the `queued` flag
exists only in `EngineBuild`. Add `app/jobs.rs::BackgroundJob<T>` with
`request`/`poll`, and use it for engine build, plugin catalog, and the file
dialog.

### A12. Config/cache JSON and HOME paths

- `shortcuts.rs:6`/`:20`/`:32` (Application Support / `.config`)
- `plugins/cache.rs:22`/`:31`
- `plugins.rs:90` `plugin_cache_path` (Library/Caches / `.cache`)
- `main.rs:312` `plugin_scan_paths`

Two “read → serde → default/error” and “pretty serialize → create dir → write”
implementations with different error policy; two platform path conventions.
Add `app/paths.rs` for all path resolution and a small `app/json_store.rs` for
shortcut config. Keep `PluginCache` typed errors but source its path from one
place.

### A13. Audio device matching and sample-rate enumeration

`audio.rs:96` device name fallback, `:162` `available_options`
(description→name/id, injects a magic common-rate list at `:182`), `:205`
`resolve_host`, `:216` `resolve_device`, `:235` `choose_config` (enumerates
supported rates independently). The UI stores the device description name as
its id, so any fallback difference silently selects the wrong device.

Add private helpers (`device_name`, `matches_device`, `output_devices`,
`supported_sample_rates`, `COMMON_SAMPLE_RATES`) and use them in
`available_options`, `resolve_device`, `start_with`, and `choose_config`. Add a
deterministic name-matching test.

### A14. Plugin descriptor classification and type-id prefixes

- `app/plugins.rs:100` and `workspace/mod.rs:882`/`:899` map
  `is_instrument → DeviceKind`
- Prefix predicate `starts_with("vst3.") || starts_with("au.")` repeated at
  `workspace/mod.rs:838`, `:863`, `main.rs:357`
- Descriptor lookup by `type_id` at `workspace/mod.rs:859`, `:905`

Add `PluginDescriptor::device_kind()` and
`notwig_plugins::is_hosted_type_id(&str)` (using `PluginFormat::prefix`), plus
one `Workspace::descriptor_for_type`. This prevents CLI/GUI scan divergence and
makes adding VST2 a one-place change.

### A15. Audio asset decode → peaks → install → prepare

`workspace/assets.rs:11` `load_assets`, `:40` `import_audio`, `:143`
`relink_asset` all do `decode → compute_peaks(..., 2048) → insert → Arc →
prepare`; bucket size repeated at `:24`, `:54`, `:148`. The UI also decodes a
file just to get duration (`shell.rs:1079`).

Add `Workspace::install_buffer(asset_id, buffer)` and a `PEAK_BUCKETS`
constant; use it in all three paths. Expose duration through
`RuntimeView`/`ProjectDoc` instead of a second decode.

### A16. Track sibling/index placement

`workspace/mod.rs:598` `group_tracks` and `:653` `duplicate_track` compute the
same parent/siblings/index block (`:622` and `:664`). Extract
`insertion_point_after(track_id) -> (Option<TrackId>, usize)` and reuse it.

---

## Part 8 — Tests

### T1. Shared fixtures

The following are rebuilt per test crate:
- `id<T>(n)` helper: `notwig-core/tests/domain_flow.rs:13`,
  `notwig-dsp/tests/graph.rs:9`, `notwig-engine/tests/render.rs:21`,
  `notwig-project/tests/bundle.rs:10`, `notwig-ui/tests/*`, etc.
- `Meta`/template document setup: `domain_flow.rs:13`, `render.rs:21`,
  `bundle.rs:10`
- Repeated `apply(...).unwrap().doc` and full `CommandPayload` literals:
  `domain_flow.rs:139` onward, `render.rs:44`, `:193`, `:239`, `:455`
- `TempoMap`/render config literals: `dsp/tests/graph.rs:26`,
  `dsp/tests/stock.rs:9`

`notwig_core::test_id!` already exists (`ids.rs:154`) but is used only by
`ids.rs` tests. Add a test-only support module (precedent:
`notwig_ui::test_api`):

```rust
// notwig-core/src/test_support.rs (cfg(test) or feature-gated)
pub fn test_id<T>(n: u128) -> Id<T>;
pub fn test_meta() -> Meta;
pub fn empty_project() -> ProjectDoc;
pub fn add_track(doc, n, kind) -> (TrackId, DeviceChainId);
pub fn apply_ok(doc, payload) -> ProjectDoc;
```

Keep it out of production APIs. A developer-only `notwig-test-support` crate is
an alternative if cross-crate access is awkward; a new crate is not required if
a feature-gated module works.

Deliberately **not** a finding: `notwig-core/tests/edit_algebra.rs`’s
`TapeModel` oracle is an independent implementation used for property testing
(`docs/EDITING_TESTING.md`). Do not factor it into the production algebra.

---

## Part 9 — Shared abstractions worth adding even without duplicates

These match the “missing abstraction” category. Several are already deferred in
`docs/CODE_STRUCTURE.md`.

1. **`TimeMus::from_beats` / `TempoMap` helpers** (C3) — one micro-beat
   constructor and one sample conversion policy.
2. **`ProjectDoc::arrangement_end()`** — currently computed four ways:
   `shell.rs:993` (`fold(8.0)+1.0`), `render.rs:12` (max or 4 beats),
   `workspace/mod.rs:1262` (same as render), `main.rs:333` (`fold(4.0)`).
   Canonicalize the minimum/fallback once.
3. **Modal window builder** (C11) — already planned.
4. **`AudioSettingsDraft`** (A6) — already planned.
5. **`BackgroundJob<T>`** (A11).
6. **`fsutil::write_atomic`** (P5) and **`paths.rs`** (A12).
7. **`ParameterIndex`** for plugin params (P4).
8. **`SoloContext` / routing view** (C1, C2) — turn graph semantics into a
   small typed view instead of repeated field matching.
9. **Device parameter accessors** (`param_value`, `param_knob`, EQ decode) so
   UI never re-derives DSP defaults (U2, U4, C5).
10. **Test support module** (T1).

---

## Part 10 — Suggested sequencing

Do these in order; each step is independently verifiable.

1. **Mechanical, low risk**
   - C8 (duplicate `UpdateSchedule`)
   - K1, K5 (duplicate-ID/validation helpers)
   - P2, P3 (format prefix, adapter lookup)
   - A5 constants: delete local `DB_MIN`/`DB_MAX` (U7) and add
     `TimeMus::from_beats` (C3) in one pass.
2. **Pure cross-crate consolidation**
   - C1 (solo), C5 dB→gain.
   - E1 (offline render loop), E4 (clip window filtering), C7/K4 (ordering).
   - S1/S2 (smoothing and stock specs) with DSP render tests.
3. **State-owner refactors**
   - C7 (clip bookkeeping), C6 (plan DTO mapping), C9 (param init).
   - C11/A1/A7/A8/A10/A11 in the app/UI shell.
4. **Correctness-critical window math**
   - C3 and C4 together, with golden tests for trim start/end, edit range,
     split, and copy/extract on sliced and unsliced clips.
5. **Semantic/architecture**
   - C2 (routing + cycle + execution order), K2 (forest ownership),
     P5–P7 (persistence), E3 (ActivePlan ownership), U4/EQ, S1 smoothing.
6. **Test support** (T1) last, so it captures the final fixture shape.

For every refactor:
- add the test at the lowest useful layer first (core algebra/property test,
  then engine render, then headless UI render);
- keep the old and new implementations side by side only long enough to prove
  parity, then delete the duplicate;
- update the crate README or `docs/CODE_STRUCTURE.md` when a boundary moves
  (notably C4, C2, P5, and the new UI widget helpers).

---

## Appendix — checked and intentionally left alone

- `notwig-core/tests/edit_algebra.rs` `TapeModel`: independent test oracle.
- `notwig-dsp/src/registry.rs` `resolve`/`resolve_latest`: shared `.get`,
  no behavioral duplication.
- EQ curve coordinate mapping (`eq.rs:41`): used once; a local `EqPlot` struct
  would improve shape but is not duplication.
- `info/audio_clip.rs` and `info/midi_clip.rs`: thin adapters, not copies.
- Per-test fixtures in `notwig-ui/tests/audio_editor.rs` and `tests/shell.rs`:
  normal per-suite setup, not cross-crate duplication.
- Stereo stride (E5): only worth changing if multichannel is planned; the
  real-time buffer layout makes it a deliberate decision.
- Random `Uuid::new_v4()` inside `apply` (`apply.rs:816`, `:863`, `:896`)
  conflicts with the “IDs are caller-supplied” rule. This is a determinism
  issue rather than duplication; track it separately as an `IdSource` design
  decision.
