# Performance Investigation: Undo/Redo, Project Open, and Track Operations

Status: investigation complete; proposed fixes below. No production code was changed.

Scope audited: `notwig-app`, `notwig-core`, `notwig-engine`, `notwig-plugins`, `notwig-project`.

Method: call-path and allocation audit of every path involved in undo/redo, project opening, and track mutation. The findings are deterministic from the code; running a profiler is still recommended before and after each fix phase (see [Validation](#validation)).

---

## TL;DR

| Symptom | Dominant root cause | Highest-leverage fix |
|---|---|---|
| Undo/redo is slow or appears to hang | History stores full `ProjectDoc` snapshots, and every undo/redo rebuilds the whole engine, which reloads every plugin | Incremental engine plan reconciliation (reuse plugin processors); delta/`Arc`-shared history; stop unconditional rebuild |
| Opening a project hangs | `Workspace::open` runs synchronously on the UI thread; every plugin is loaded twice (metadata + engine); every audio asset is decoded eagerly | Async open with progress; cache plugin parameter metadata; lazy/background asset decode |
| Moving/deleting tracks is incredibly slow | `MoveTrack`/`DeleteTrack` are marked "structural", forcing a full engine rebuild (all plugins reloaded); each edit also makes 2-3 full document clones and repeated O(n²) validation | Reuse plugin instances in the engine; eliminate redundant clones; validate once per batch |

The single largest cost is **plugin instantiation during `rebuild_engine`**. In a project with hosted plugins, moving one track currently unloads and reloads every plugin in the project.

---

## Cost model (why these operations are expensive)

`ProjectDoc` is a deep value type (`crates/notwig-core/src/domain.rs:195`). A clone copies:

- every `Track` and send,
- every clip, including all MIDI notes and audio slices,
- every device with its parameter map and state reference,
- the asset index, routing graph, arranger lanes, and track forest.

A "structural" command goes through this path:

```text
Workspace::apply
  -> clone doc for undo                          (workspace.rs:213)
  -> notwig_core::apply
       -> clone doc again for transactionality   (apply.rs:49)
       -> mutate
       -> PlaybackSchedule::from_doc (clones every clip) (apply.rs:54)
       -> doc.validate()                         (apply.rs:597, etc.)
  -> rebuild_engine
       -> compile (validates doc again)          (graph.rs:92)
       -> new PluginHost + new Engine            (workspace.rs:180-193)
       -> Engine::prepare
            -> ActivePlan::build
                 -> factory.create per device
                      -> host.load(plugin)       (factory.rs:88)
                 -> processor.prepare (activate plugin)
```

So one track move with N plugins can mean: 2 full document clones, 2 full validations, one DSP graph compile, N plugin unloads, N plugin loads, N plugin activations, and dropping the old engine — all on the UI thread.

---

## 1. Undo/Redo

### 1.1 Full-document snapshots

`History` stores whole `ProjectDoc` values:

```rust
// crates/notwig-app/src/history.rs:5
pub struct History {
    undo: Vec<ProjectDoc>,
    redo: Vec<ProjectDoc>,
}
```

`record` receives a clone; `undo`/`redo` additionally clone the current document into the opposite stack:

```rust
// crates/notwig-app/src/history.rs:12
pub fn record(&mut self, before: ProjectDoc) { ... }
// crates/notwig-app/src/history.rs:20
pub fn undo(&mut self, current: &ProjectDoc) -> Option<ProjectDoc> {
    let previous = self.undo.pop()?;
    self.redo.push(current.clone());
    Some(previous)
}
```

Consequences:

- Every edit that can be undone makes at least two deep document copies (the history snapshot and the transactional copy inside `apply`).
- `MAX_UNDO = 200` (`history.rs:3`) means up to 200 full documents in memory, including every note and audio slice. Large sessions can consume hundreds of MB or more.
- `record` trims with `self.undo.remove(0)` (`history.rs:15`), which shifts the entire vector. This is minor compared with the clones but should be a `VecDeque`.

### 1.2 Unconditional engine rebuild on undo/redo

```rust
// crates/notwig-app/src/workspace.rs:240
pub fn undo(&mut self) -> Result<(), String> {
    let Some(previous) = self.history.undo(&self.doc) else { return Ok(()); };
    self.doc = previous;
    self.dirty = true;
    self.rebuild_engine()?;   // always
    ...
}
```

`redo` does the same (`workspace.rs:252`). Undoing a rename, a comment, or a mixer value still recompiles the graph and reloads every plugin. The code does not track whether the undone change was structural.

### 1.3 Every edit is snapshotted, including continuous gestures

`Workspace::apply` records an undo snapshot for everything except transport commands:

```rust
// crates/notwig-app/src/workspace.rs:209
let record_undo = !matches!(
    payload,
    CommandPayload::SetTransport { .. } | CommandPayload::Pause | CommandPayload::Stop
);
let before = record_undo.then(|| self.doc.clone());
```

This includes `SetParam` (plugin editor streams parameter changes) and `SetTrackMixer` (fader drags). Dragging a fader or automating a plugin knob can therefore allocate a full document clone per UI event and fill the undo stack with micro-edits. This compounds the lag during gestures.

### 1.4 Batch operations clone once per command

`apply_batch` already clones the document for the batch snapshot and again as the working document:

```rust
// crates/notwig-app/src/workspace.rs:1048
let before = self.doc.clone();
let mut doc = self.doc.clone();
for payload in payloads {
    let result = apply(&doc, &Command::new(payload))...; // apply clones again
    doc = result.doc;
}
```

`notwig_core::apply` always starts with `let mut next = doc.clone()` (`apply.rs:49`), so a batch of K commands performs `2 + K` full document clones. Time-selection edits and track grouping duplicate the same pattern.

### 1.5 No dirty/schedule awareness

`apply` unconditionally rebuilds `PlaybackSchedule`, cloning every clip and note payload:

```rust
// crates/notwig-core/src/apply.rs:48
let mut next = doc.clone();
...
engine_msgs.push(EngineMsg::UpdateSchedule {
    schedule: Arc::new(PlaybackSchedule::from_doc(&next)),
});
```

`PlaybackSchedule::from_doc` (`protocol.rs:342`) clones every clip. This happens for tempo changes, mixer changes, device parameter changes, and undo snapshots, even though the schedule did not change. For a MIDI-heavy project this is a hidden per-edit O(total notes) clone.

---

## 2. Opening a Project (apparent hang)

### 2.1 Synchronous open on the UI thread

Opening from the menu calls `Workspace::open` directly inside `apply_dialog` on the UI thread:

```rust
// crates/notwig-app/src/gui.rs:693
match Workspace::open(&path) { ... }
```

Initial startup does the same before the first frame (`gui.rs:53-56`), so the window does not appear until the open completes. `Workspace::open` is a single blocking function covering bundle parsing, plugin metadata loading, asset decoding, graph compilation, and engine construction.

### 2.2 Each plugin is loaded twice on open

`from_doc` calls `refresh_plugin_specs()` and then `rebuild_engine()`:

```rust
// crates/notwig-app/src/workspace.rs:111
workspace.refresh_plugin_specs();
workspace.load_assets();
workspace.rebuild_engine()?;
```

`refresh_plugin_specs`/`ensure_plugin_spec` load a throwaway plugin instance just to enumerate parameters:

```rust
// crates/notwig-app/src/workspace.rs:713
let mut host = new_plugin_host();
let Ok(instance) = host.load(&descriptor) else { return; };
let params = instance.describe_parameters();
```

Then `rebuild_engine` -> `build_engine` -> `Engine::prepare` loads every plugin again for audio (`factory.rs:88`, `engine.rs:222-240`, `runtime.rs:82-120`). Hosted plugins are typically the slowest thing in the app to construct, so this doubles the worst part of open time. It also means the metadata probe can trip the plugin failure/blacklist counter (`host.rs:130-140`) for a plugin that would otherwise load in the engine.

### 2.3 All audio assets are decoded eagerly

```rust
// crates/notwig-app/src/workspace.rs:117
fn load_assets(&mut self) {
    for entry in self.doc.assets.assets.values() {
        ... notwig_project::decode_audio(&path) ...
        self.peaks.insert(entry.id, compute_peaks(&buffer, 2048));
    }
}
```

Every audio file in the bundle is fully decoded and peak-scanned on the UI thread during open. Large sample-based projects can spend seconds here, before any plugin is even reloaded.

### 2.4 The background scan completion re-freezes the UI

`poll_plugin_catalog` runs every frame (`gui.rs:789`). When the background scanner finishes, it synchronously refreshes plugin specs and rebuilds the engine:

```rust
// crates/notwig-app/src/workspace.rs:668
if !PluginCatalog::global().poll() { return false; }
self.plugin_descriptors = ...;
self.refresh_plugin_specs();      // may load plugins
if ... contains plugin devices ... {
    let _ = self.rebuild_engine(); // reloads all plugins
}
```

This produces a second UI freeze shortly after startup, when the scan thread delivers results. It is easy to misattribute this to "opening the project" because it happens near the same time.

### 2.5 Audio restart is also synchronous

After a successful open, `restart_audio` tears down and reopens the audio device (`gui.rs:80-88`, `audio.rs:26-68`). If the device does not support the project rate, this fails visibly, but the teardown/restart itself is another blocking step on the UI thread.

---

## 3. Moving and Deleting Tracks

### 3.1 Both commands are treated as DSP-structural

```rust
// crates/notwig-core/src/protocol.rs:216
pub fn requires_plan_rebuild(&self) -> bool {
    matches!(self,
        Self::SetTempo { .. }
        | Self::SetTimeSignature { .. }
        | Self::SetTrackActive { .. }
        | Self::CreateTrack { .. }
        | Self::DeleteTrack { .. }
        | Self::MoveTrack { .. }
        ...
```

`Workspace::apply` responds by calling `rebuild_engine` (`workspace.rs:221-223`), which:

- builds a brand-new `PluginHost`,
- compiles a new plan (and validates the document again),
- instantiates and activates every plugin in the project,
- swaps and drops the old engine.

A reorder within the same parent changes no audio routing and no plugin topology, yet it reloads every plugin. A delete only removes a subtree, but still reloads all surviving plugins.

### 3.2 Double cloning and repeated validation

`DeleteTrack` and `MoveTrack` each end with `doc.validate()` (`apply.rs:640`, `apply.rs:694`). `compile` then validates the same document again (`graph.rs:92`). Validation is not cheap:

- `ProjectDoc::validate` walks all tracks, chains, devices, lanes, and clips (`domain.rs:257`).
- `TrackForest::validate` calls `parent_of` for every track; `parent_of` scans all child lists (`tracks.rs:237-241`, `tracks.rs:270-333`), making this O(n²).
- `validate_routing` walks the output chain from every track (`apply.rs:697`), worst-case O(n²).
- The second lane/clip loop uses `lane.clips.contains(&clip.id)` (`domain.rs:339`), O(lane length) per clip.

None of this is the dominant cost when plugins exist, but it becomes noticeable on large projects and multiplies across batch operations.

### 3.3 Grouping/duplicating tracks issues one apply per step

`group_tracks` (`workspace.rs:439-492`) and `duplicate_track` (`workspace.rs:494+`) perform a sequence of separate `apply` calls. Each structural step rebuilds the engine and reloads plugins:

```rust
// group_tracks
let group = self.create_track_with_parent(...)?;   // rebuild
for track in selected {
    self.apply(CommandPayload::MoveTrack { ... })?; // rebuild per track
}
```

A group of 8 tracks can trigger 9 full plugin reload cycles and 18+ full document clones for what should be one atomic edit.

### 3.4 The track-move gesture itself is fine; the commit is not

The UI only computes a drop target during the drag and emits `MoveTrack` on release (`arranger.rs:621-645`). The feather-in-the-hand lag is therefore not in the drag loop; it is in the commit path described above. (If dragging feels slow *while* moving, that is a separate frame-time issue, but the code shows no per-frame command emission.)

---

## 4. Cross-Cutting Issues

### 4.1 The old engine is dropped while holding the engine mutex

```rust
// crates/notwig-app/src/workspace.rs:167
*self.engine.lock().map_err(|err| err.to_string())? = engine;
```

The assignment drops the previous `Engine`, including deactivating and unloading every plugin, while the mutex is held. The audio callback uses `try_lock` (`audio.rs:133`) so it degrades to silence, but the UI thread can block for the duration of plugin teardown. Moving the old engine out of the lock and dropping it on a worker avoids a visible stall.

### 4.2 Debug builds magnify all of this

The workspace has no dedicated dev profile tuning. Deep clones, serde allocation, and plugin wrapper work are substantially slower in debug. Some of the perceived "hang" may disappear in `--release`, but the algorithmic issues above remain.

---

## Solutions

The fixes are ordered by impact per unit of risk. Phase 1 and Phase 2 address all three reported symptoms; Phase 3 removes the remaining structural headroom cost.

### Phase 0: Measure first (small effort)

Add a small benchmark harness before changing behavior:

- `ProjectDoc::clone` with representative 32 tracks / 500 clips / 20k notes.
- `apply` + `apply_batch` with 1, 10, and 100 commands.
- `rebuild_engine` with a stub `DeviceFactory` and with a counting factory that records `create` calls.
- `Workspace::open` on a fixture bundle containing audio assets and a stub plugin or two.

Assertions that should hold after fixes: zero `DeviceFactory::create` calls when moving a track; one clone per user gesture; one schedule rebuild per clip-changing batch.

### Phase 1: Remove redundant work without redesign (low risk, high impact)

**1. Stop double-cloning in `apply`.**

Add an in-place entry point to `notwig-core`:

```rust
// notwig-core
pub fn apply_in_place(doc: &mut ProjectDoc, command: &Command)
    -> Result<Vec<EngineMsg>, CoreError>;
```

Keep the existing `apply(&ProjectDoc, ...)` as a thin wrapper for callers that need transactional behavior. In `Workspace::apply`, take/keep the undo snapshot first, then mutate `self.doc` in place. On error, restore the snapshot. This cuts the per-edit clone count from 2 to 1 (or 0 once history is delta-based).

For `apply_batch`, use one working document and one `apply_in_place` per command; no per-command clone. `lower_and_apply` benefits automatically.

**2. Only record undo for discrete edits; coalesce continuous ones.**

- Do not snapshot `SetParam` and `SetTrackMixer` per event.
- Introduce begin/end gesture boundaries from the UI (fader drag, plugin knob drag) and record one snapshot per gesture using the value at gesture start.
- At minimum, coalesce consecutive commands of the same kind/target within a short window (e.g. 250 ms) into one history entry.

This removes most undo-stack churn and memory growth during normal editing.

**3. Stop rebuilding the playback schedule when clips/notes did not change.**

Add a `schedule_changed` signal to the apply result (or split commands into groups). Only emit `EngineMsg::UpdateSchedule` when a clip, note, or clip payload was added, removed, or edited. For batch operations, emit one update at the end.

Expected effect: every mixer/parameter/rename edit becomes O(touched entities) instead of O(all notes), and undo snapshots stop paying schedule cost on non-clip edits.

**4. Validate once per batch, not per command.**

- Add an internal `apply_in_place_unchecked` used between commands inside a batch; run `doc.validate()` once before publishing the batch.
- In `apply`, avoid calling `doc.validate()` in both the mutation helper and again in `compile` for the same operation.
- Fix the obvious quadratic validation helpers:
  - build a parent map once in `TrackForest::validate` instead of calling `parent_of` per track;
  - replace `validate_routing`'s per-track chain walk with a color/DFS cycle check over the routing graph;
  - replace `lane.clips.contains(&clip.id)` with a set or a single pass that validates both directions.

**5. Use `VecDeque` for the history cap.**

Replace `self.undo.remove(0)` with `VecDeque::pop_front` (`history.rs:15`). Minor, but free.

Expected Phase 1 result: undo/redo and ordinary edits stop doing unnecessary full-document and full-schedule work; undo/redo of non-structural edits no longer reloads plugins.

### Phase 2: Incremental engine updates (the main fix)

**1. Reconcile the plan instead of rebuilding the engine.**

`Engine::apply(EngineMsg::ApplyPlan)` currently replaces the plan and calls `prepare()`, which constructs a fresh `ActivePlan` and calls `DeviceFactory::create` for every device (`engine.rs:134-141`, `engine.rs:222-240`). Add reconciliation:

```rust
impl ActivePlan {
    fn reconcile(
        &mut self,
        new_plan: &Arc<CompiledPlan>,
        new_schedule: Arc<PlaybackSchedule>,
        factory: &dyn DeviceFactory,
    ) -> Result<(), EngineError>;
}
```

Rules:

- Preserve a processor when the new plan contains the same `DeviceId` with the same `type_id` and the same child-chain topology.
- Instantiate only devices that are new (or whose subtree changed).
- Drop processors only for removed/changed devices.
- Rebuild track runtimes and processing order from the new plan; do not touch surviving processors.
- Reuse `Arc` assets and the existing `PluginHost`; never construct a new host for a document edit.

Expose this through `Workspace::rebuild_engine` as `sync_engine_plan` for structural changes. `rebuild_engine` then becomes a full rebuild only for sample-rate/config changes or explicit recovery, not for ordinary edits.

Expected effect: moving/deleting tracks, grouping, undo/redo, and device add/remove no longer reload surviving plugins. This is the largest single win and directly fixes all three reported symptoms.

**2. Make `PluginDeviceFactory` reusable and instance-aware.**

Even with reconciliation, keep one long-lived `PluginHost`/factory per engine lifetime. Add:

- a map `DeviceId -> live processor` (or let `ActivePlan` own it, as above),
- a stable device signature for reuse decisions (`type_id`, child chains, spec identity),
- a `release(device_id)` path so removed devices are dropped off the UI thread.

Avoid `new_plugin_host()` inside every `build_engine` (`workspace.rs:180`); it discards any host-level caches and guarantees cold plugin loads.

**3. Move engine swap and drop off the critical path.**

Swap the new engine under the lock, take the old engine out, and drop it on a worker thread:

```rust
let old = std::mem::replace(&mut *self.engine.lock()?, engine);
std::thread::spawn(move || drop(old));
```

This prevents plugin deactivation/unload from blocking the UI (see 4.1).

**4. Split structural flags by impact.**

`requires_plan_rebuild` conflates several concerns (`protocol.rs:216`). Introduce:

- `changes_graph` (devices, routing, track add/remove, parent change),
- `changes_track_order` (reorder only; no DSP topology change),
- `changes_schedule` (clips/notes),
- `changes_transport`.

`MoveTrack` within the same parent should update order without touching DSP. `SetTempo`/`SetTimeSignature` may need a plan rebuild for tempo-dependent devices but should still reuse plugin instances. `SetTrackActive` needs a routing/processing update, not a rebuild.

Expected Phase 2 result: track move/delete and undo/redo become UI-scale operations even in plugin-heavy projects.

### Phase 3: Make history cheap (medium effort)

**Option A (smaller change): structural sharing via `Arc`.**

Wrap the large immutable payloads so `ProjectDoc::clone` becomes shallow for the expensive parts:

- `MidiClipData.notes: Arc<[NoteEntity]>` (or `Arc<Vec<NoteEntity>>`), mutated with `Arc::make_mut`;
- `AudioClipData.slices: Arc<[AudioSlice]>`;
- clip payloads themselves as `Arc<ClipPayload>`.

Then a document snapshot is O(clips + devices) pointer copies instead of O(notes + slices). This makes the existing snapshot history viable and speeds up every clone in `apply`/`apply_batch`.

**Option B (longer change): command/delta history.**

Store `CommandPayload` plus its inverse (or a compact `DocChange` journal) instead of full documents. `ApplyResult` already produces `StateDelta`; add an `invert` operation alongside `apply` and store reversal commands. Destructive edits (delete track, group) can still use a sparse snapshot of the removed subtree. This gives constant-size history entries and avoids copying untouched data.

A hybrid is reasonable: Option A now, Option B when the editor needs redo of complex selections.

**Also:** cap history by estimated bytes, not just entry count, and skip recording no-op commands.

### Phase 4: Async project open and asset loading

**1. Move `Workspace::open` off the UI thread.**

Add an async open request that returns a channel:

```text
open_async(path) -> Receiver<Result<Workspace, String>>
```

While pending, show the main window with a loading state and progress ("Loading plugins 2/5", "Decoding assets 3/12"). On success, swap the workspace, restart audio, and repaint. On failure, keep the current project intact.

Caveat: some plugin formats require instantiation on the main thread on macOS. If the backend has that constraint, load plugin instances on the main thread but **incrementally** (one per frame or a small budget per frame) with progress, instead of blocking for all of them. The bundle/asset parsing and audio decoding can always move off-thread.

**2. Cache plugin parameter metadata.**

`PluginDescriptor`/`PluginCache` should carry the parameter schema discovered during scan, or `ensure_plugin_spec` should be deferred until a device editor actually needs it. Then opening a project does not instantiate plugins just to build `DeviceSpec`s. If a probe is unavoidable, load at most once and reuse the instance for the engine rather than loading twice.

**3. Lazy/background asset decoding.**

- Store asset metadata and peaks in the bundle/cache; decode only when a clip first needs the buffer.
- Alternatively decode in a background worker and send `EngineMsg::PrepareAudioAsset` as each asset becomes ready. Playback of not-yet-ready assets can be silent with a status indicator.
- Compute peaks in the same worker.

Expected result: the window appears immediately; plugin load happens with visible progress; asset-heavy projects no longer block on open.

**4. Do not rebuild at the end of the background scan unless the plan actually changed.**

`poll_plugin_catalog` should hand off the descriptor refresh to a worker or apply only new/removed descriptors incrementally. If no device in the current document is affected, skip the engine work entirely.

### Phase 5: Gesture and UI-level reductions

- Coalesce fader/knob gestures (see Phase 1.2). Consider not routing every parameter tick through the full command path while a drag is active; send a lightweight `SetParam` directly to the engine and commit one document edit when the gesture ends.
- Debounce autosave and run it off-thread; `autosave_if_due` serializes the full document on the UI thread (`workspace.rs:277-290`). A large project will hitch every 60 seconds.
- Avoid cloning `PlaybackSchedule` and the full runtime snapshot for the UI more often than necessary.

---

## Suggested implementation order

1. Phase 0 benchmarks.
2. Phase 2.1/2.2 engine plan reconciliation + reusable host (largest win; fixes all three symptoms).
3. Phase 1.1/1.2/1.3 clone, gesture, schedule fixes.
4. Phase 1.4/1.5 validation and history-cap fixes.
5. Phase 4 async open + metadata cache + lazy assets.
6. Phase 3 history structural sharing/deltas.
7. Phase 5 gesture/autosave polish.

Phase 2 can land independently of Phase 1 and will by itself make track move/delete and undo/redo usable. Phase 1 reduces the cost of every edit and is a prerequisite for good open-time behavior under load.

---

## Validation

Add reproducible checks rather than relying on feel:

1. **Clone benchmark**: `ProjectDoc::clone` on a 32-track / 500-clip / 20k-note fixture. Target: dominated by clip/device count, not note count, after Phase 3.
2. **Apply benchmark**: one `SetTrackMixer`, one note edit, one 100-command batch. Assert one clone per user action and one schedule rebuild per clip-changing batch.
3. **Engine reuse test**: a `CountingFactory` that records `create` calls. Moving a track and undoing the move must make zero new `create` calls for existing devices; deleting a track must create zero and release only the deleted subtree.
4. **Open benchmark**: fixture bundle with audio assets and stub plugin devices. Measure time to first frame and time to playable engine; assert UI responsiveness with a max-frame-time threshold during open.
5. **Manual checks**: open a project with 3+ VST3/AU instruments, drag-reorder tracks, delete a group, undo/redo; the UI should not drop below interactive frame rate and the audio thread should not underrun beyond the existing fade policy.

---

## Appendix: Primary code references

| Concern | Location |
|---|---|
| Full document snapshots | `crates/notwig-app/src/history.rs:5-30` |
| Undo/redo always rebuild engine | `crates/notwig-app/src/workspace.rs:240-261` |
| Undo snapshot on every edit | `crates/notwig-app/src/workspace.rs:207-226` |
| Core `apply` clones document | `crates/notwig-core/src/apply.rs:48-59` |
| Batch clones per command | `crates/notwig-app/src/workspace.rs:1044-1076` |
| Full engine rebuild + plugin reload | `crates/notwig-app/src/workspace.rs:140-206` |
| Plugin factory loads on every create | `crates/notwig-engine/src/factory.rs:82-91` |
| Engine rebuilds runtime from scratch | `crates/notwig-engine/src/engine.rs:222-240` |
| Runtime instantiates every device | `crates/notwig-engine/src/runtime.rs:82-120` |
| Structural command classification | `crates/notwig-core/src/protocol.rs:216-235` |
| Track move/delete + validation | `crates/notwig-core/src/apply.rs:600-694` |
| Quadratic validation helpers | `crates/notwig-core/src/apply.rs:697-723`, `crates/notwig-core/src/tracks.rs:237-333`, `crates/notwig-core/src/domain.rs:257-353` |
| Schedule rebuilt on every command | `crates/notwig-core/src/apply.rs:54`, `crates/notwig-core/src/protocol.rs:342-360` |
| Synchronous open from UI | `crates/notwig-app/src/gui.rs:53-65`, `crates/notwig-app/src/gui.rs:693-703` |
| Plugin metadata probe (extra load) | `crates/notwig-app/src/workspace.rs:698-746` |
| Eager asset decode on open | `crates/notwig-app/src/workspace.rs:117-138` |
| Scan completion rebuild | `crates/notwig-app/src/workspace.rs:668-684` |
| Group/duplicate sequential applies | `crates/notwig-app/src/workspace.rs:439-492` |
| Engine swap/drop under mutex | `crates/notwig-app/src/workspace.rs:167` |
| Autosave serializes on UI thread | `crates/notwig-app/src/workspace.rs:277-290` |
| Planned async workers / off-RT loads | `plan/crates/notwig-app/APP.md:12`, `plan/crates/notwig-plugins/PLUGINS.md:18-21` |
