# Audio / DSP Analysis: Stability, Builtin Devices, Import, and Export

Status: investigation complete; findings and proposed solutions below. No production code was changed.

Scope: `notwig-engine`, `notwig-dsp`, `notwig-plugins`, `notwig-project`, and the audio paths in `notwig-app`.

Method: call-path and allocation audit of the real-time render path, plugin lifecycle, import pipeline, and export pipeline. This is static analysis; use the validation plan at the end to measure before and after each change.

---

## Executive Summary

| Question | Short answer |
|---|---|
| Is the DSP stable? | The basic graph execution is sound and deterministic, but the current runtime is **not fully RT-safe** and has several crash/dropout risks: it allocates in the audio callback, can panic when the device block size exceeds 512 frames, and has no protection against a plugin that hangs inside `process`. |
| Can a slow plugin hang the app? | **Yes.** Plugin metadata loading still happens on the UI thread during open/insert, and the background engine-build thread has no timeout or cancellation. A plugin that never returns from `load`/`activate` hangs that path forever. A plugin that hangs in `process` stalls the audio callback. |
| Are the builtin devices efficient? | Reasonably, but not optimal. `Tool` performs pan `cos`/`sin` plus eight smoothed reads per sample. `EQ` recomputes all four biquad coefficient sets on every block even when nothing changed. `Chain` is allocation-free in steady state. |
| Why does importing a long MP3 hang? | Import is fully synchronous on the UI thread. The high-quality resampler allocates a `Vec` **per output frame**, then the pipeline makes several full-buffer copies, writes a large WAV, re-reads it to hash it, and scans it for peaks. A 5-minute 44.1 kHz file can cause tens of millions of heap allocations. |
| Should import be offloaded? | **Yes.** Offloading fixes the hang; the resampler and copy/hash work must also be fixed or the background import will still be unnecessarily slow. |
| Are FLAC/OGG supported? | Decoding is already wired through Symphonia and the file picker accepts them, but there are no fixtures/tests and detection is extension-only. See [Supported Formats](#supported-formats-wav-mp3-flac-ogg). |
| What about export? | Export currently writes WAV only, always from time 0 to the last clip, with no dialog. A format/range popup is a good next step; add streaming encoders and background rendering. |

---

## 1. DSP Stability

### 1.1 What is already sound

- Graph compilation validates the document, resolves devices, computes processing order, and rejects cycles (`crates/notwig-dsp/src/graph.rs:87-292`; processing order at `crates/notwig-engine/src/runtime.rs:648-707`).
- The render loop advances by a non-zero segment even when looping, so there is no obvious infinite loop in `Engine::process` (`crates/notwig-engine/src/engine.rs:283-338`).
- Audio asset rendering handles mono/stereo and clamps reads beyond the source to silence (`crates/notwig-engine/src/runtime.rs:607-630`).
- Missing/unloadable plugins do not crash the graph; `build_processor` simply omits them (`crates/notwig-engine/src/runtime.rs:709-741`).
- Continuous parameters in stock devices use smoothing, which avoids most zipper noise.

### 1.2 RT-safety and dropout risks

The engine is behind `Arc<Mutex<Engine>>`. The audio callback uses `try_lock` (`crates/notwig-app/src/audio.rs:133`), so it will not block, but it outputs silence whenever the UI thread holds the lock. Engine install, parameter updates, schedule updates, and track-state updates all contend for this lock. That means:

- frequent parameter edits from a plugin editor can cause audible dropouts;
- `install_built_engine` swaps and drops the old engine while holding the lock (`crates/notwig-app/src/workspace.rs:342-355`), potentially blocking the UI/audio handoff for the duration of plugin teardown;
- the audio callback allocates a new `Vec` for drained events on every callback (`crates/notwig-app/src/audio.rs:138`), and `scratch.resize` can allocate if the device block size grows (`audio.rs:130`);
- the render path allocates every block:
  - `self.order.clone()` (`runtime.rs:236`),
  - `self.chains.get(&chain).cloned()` per track (`runtime.rs:241`),
  - `Vec<NoteEvent>` maps in `collect_events` (`runtime.rs:353-448`),
  - `to_vec()` for sends and main routing (`runtime.rs:495-497`, `runtime.rs:518`),
  - `vec![0.0; frames * 2]` for PDC delay (`runtime.rs:531`).

None of these are guaranteed to fail, but they violate the "no allocation/locks on the RT thread" contract in `plan/crates/notwig-app/APP.md` and `plan/crates/notwig-engine/ENGINE.md`. Under load they cause xruns and unpredictable latency.

### 1.3 Buffer-size mismatch can panic

`TrackRuntime` allocates its buffers from `config.max_block_size` (`crates/notwig-engine/src/runtime.rs:30-40`). The workspace builds engines with `max_block_size: 512` (`crates/notwig-app/src/workspace.rs:235-253`). The audio callback computes `frames = data.len() / channels` and passes it straight to `engine.process` without checking it against the configured maximum (`crates/notwig-app/src/audio.rs:126`, `crates/notwig-engine/src/engine.rs:256-262`).

If the device chooses a larger buffer (1024, 2048, or a larger default on some hardware), `runtime.render` slices `track.buffer[..frames * 2]` and panics. On macOS the default is often 512, but it is not guaranteed, and changing the audio device can produce a different size. This is a real crash path.

**Fix:** either size all runtime buffers to the device maximum and pass that size into `EngineConfig`, or make `Engine::process` split `frames` into chunks of `config.max_block_size`. Also validate the output length against `frames * channels`.

### 1.4 Plugin load and plugin process can hang without recovery

There are three distinct hang scenarios:

1. **UI-thread metadata load.** `ensure_plugin_spec` creates a `PluginHost`, loads an instance, and drops it just to read parameters (`crates/notwig-app/src/workspace.rs:912-952`). This is called from `from_doc` during open (`workspace.rs:141-169`), from `insert_plugin`, and from `poll_plugin_catalog` on the UI thread. A plugin that blocks in `load` or `describe_parameters` freezes the UI/opening path indefinitely.
2. **Background engine build.** `request_engine_rebuild` spawns `build_engine_from_request` on a worker (`workspace.rs:256-298`), which loads and activates every plugin through `PluginDeviceFactory::create` (`crates/notwig-engine/src/factory.rs:81-91`). There is no per-plugin timeout and no cancellation. If one plugin hangs, the worker never finishes, `engine_building()` stays true forever, queued rebuilds never run, and the status remains "Loading instruments and effects..." (`workspace.rs:256-298`). The app remains responsive but silent.
3. **RT process.** `RackPluginInstance::process` calls directly into the plugin (`crates/notwig-plugins/src/rack.rs:100-175`). If the plugin hangs, the audio callback never returns and the output stream stalls. There is no watchdog and no out-of-process isolation (deferred in v0 by design).

The plan already requires load/scan to run off-RT and to have per-plugin timeouts (`plan/crates/notwig-plugins/PLUGINS.md:18-26`). The implementation only partially satisfies the threading part and has no timeout.

Also note that a background-loaded plugin instance is later used on the audio thread. Some plugin APIs, notably Audio Units, have main-thread affinity for instantiation or GUI work. If the backend requires the main thread, moving instantiation wholesale to a worker is not safe. The backend must declare that constraint so the app can schedule accordingly.

**Recommended handling:**

- Never call `ensure_plugin_spec` on the UI thread during open. Move metadata probing to the background, or store the parameter schema in the plugin cache produced during scanning.
- Put a wall-clock budget around each plugin load/activate. On timeout, log it, mark the device unresolved, blacklist after repeated failures, and continue building the rest of the graph. A safe fallback is silence for that device (the plan explicitly allows this).
- Because a hanging thread cannot be safely killed in-process, either run plugin probing/loading in the scanner subprocess with a timeout that the parent kills, or accept an abandoned thread but keep the rest of the app responsive. Out-of-process hosting is the long-term robust answer.
- Keep plugin activation (`prepare`) in the same supervised context as loading; it can hang too.
- Add a status/error surface for "plugin X did not respond" instead of a permanent loading state.
- Do not drop the old engine while holding the engine mutex; move it out and drop it on a worker.

### 1.5 Stuck notes when stopping

When playback is stopped, `Engine::process` calls `runtime.emit_all_notes_off()` (`crates/notwig-engine/src/engine.rs:271-276`), which simply clears the active-note map (`crates/notwig-engine/src/runtime.rs:162-166`). It does **not** generate `NoteEvent::Off` events for the notes that were active. Plugins therefore may keep notes sounding after Stop.

The loop-jump/panic path does the right thing: it sets `panic_pending` and `collect_events` emits note-offs (`runtime.rs:353-378`). The stopped-transport path bypasses that mechanism.

**Fix:** turn `emit_all_notes_off` into the panic path (set `panic_pending`, let the next block collect and send note-offs), or send `AllNotesOff` to every processor that supports it. Do not clear tracking without informing the instruments.

### 1.6 Note-off boundary can land one sample past the block

In `collect_events`, note-on events require `note_start < block_end`, but note-off events allow `note_end <= block_end` (`runtime.rs:405-437`). A note ending exactly at `block_end` produces `sample_offset == frames`, which is outside the block's sample range. The event is forwarded to the plugin with that offset (`crates/notwig-plugins/src/rack.rs:277-297`). Depending on the plugin, this can be ignored, cause an off-by-one, or be applied to the next block incorrectly.

**Fix:** either clamp the offset to `frames - 1` for the current block or defer that off event to the next block, and document the event boundary convention.

### 1.7 Bypass and latency compensation gaps

- `build_processor` returns `None` for bypassed devices (`runtime.rs:713-717`). The device disappears from the runtime, but `CompiledPlan` still includes its latency in `latency_samples` (`crates/notwig-dsp/src/graph.rs:159`, `graph.rs:218-229`). The PDC delay computation therefore compensates for a device that is not running, shifting the bypassed track in time.
- `RackPluginInstance::latency()` always returns 0 (`crates/notwig-plugins/src/rack.rs:175-177`), and `capabilities().reports_latency` is `false` (`rack.rs:181-191`). Hosted plugin latency is not represented in the plan, so PDC cannot align plugin-instrument/effect paths. The plan requires latency reporting when non-zero (`PLUGINS.md:28-30`).
- Bypass should ideally keep the processor alive but bypass its processing while retaining latency, or explicitly remove its latency from the plan.

### 1.8 Denormals, NaN, and gain changes

- The EQ uses `f64` biquads with no denormal protection (`crates/notwig-dsp/src/stock.rs:580-587`). Long quiet tails can produce denormals and CPU spikes on x86-class hardware; Apple Silicon is less affected but the code is portable.
- Plugin output is not checked for NaN/Inf. A misbehaving plugin can poison the mix permanently.
- Track volume/pan changes are applied as a constant gain per block (`runtime.rs:450-464`); there is no smoothing, so mute/solo/fader moves can click.
- There is no master limiter or NaN guard; that is acceptable for v0 but should be explicit.

**Fix:** add flush-to-zero/denormal guards around recursive filters and plugin buffers; sanitize NaN/Inf to zero at the plugin boundary; add short gain ramps for track gain/pan; document clipping policy.

### 1.9 Plugin state is not restored

`DeviceInstance.state_ref` is persisted (`crates/notwig-core/src/domain.rs:166-176`) and copied during duplication (`crates/notwig-app/src/workspace.rs:820`), but the engine never reads the state blob or calls `load_state` on the plugin. Playback and export therefore use default plugin state unless the user manually restores it. This is a correctness issue for both reopen and export, and it makes exported output unreliable.

**Fix:** store state blobs in the bundle (the `state/` directory already exists in the bundle layout), load them into the engine after instantiation, and apply them before rendering/export. Track a format/UID/version compatibility check as `load_state` already does (`crates/notwig-plugins/src/rack.rs:206-219`).

---

## 2. Builtin Devices (`notwig-dsp`)

### 2.1 Tool

`ToolProcessor::process` runs per frame and does:

- eight `SmoothedValue::next()` calls,
- `Pan(pan).gains()`, which computes `cos` and `sin`,
- mid/side, width, swap, and polarity math (`crates/notwig-dsp/src/stock.rs:368-402`).

The pan gains are recomputed every sample even though the pan value is slowly smoothed. At 48 kHz that is 96,000 trig calls per second per Tool instance. A project using Tool on every track will spend a noticeable amount of CPU on trig alone.

**Optimizations:**

- Smooth the left/right pan gains instead of the raw pan value, and update them once per block (or every N samples).
- Compute constant block gains for input/output/width/dry-wet and apply them per frame; keep per-sample smoothing only where click avoidance actually requires it.
- Hoist the polarity and swap state changes out of the per-frame branch when they are stable.

### 2.2 EQ

`EqProcessor::process` calls `update_band` for all four bands on **every block** (`stock.rs:703-711`). Each update calls `Biquad::set` (`stock.rs:514-578`), which performs `cos`, `sin`, `powf`, clamps, and builds a new coefficient set. This happens even when no parameter changed and even for disabled bands. Then each sample runs four biquads per channel with coefficient lerp and an enable mix.

The EQ is correct in structure but wasteful:

- Recompute coefficients only when a band parameter actually changes. `set_param` can mark the band dirty and `process` can rebuild at block start (`stock.rs:695-701`).
- Skip fully disabled bands whose enable mix has reached zero; currently they still run the filter math.
- After a coefficient ramp finishes, avoid per-sample lerp; use a block-constant coefficient.
- Consider f32 processing for the biquad states if numerical stability permits, or keep f64 and accept the cost; benchmark both.
- Add denormal flushing (see 1.8).

### 2.3 Chain

`ChainProcessor` is allocation-free in steady state: the dry buffer is allocated in `prepare` and only resized if the block grows (`stock.rs:766-842`). It copies the dry signal once, processes children in place, then mixes. Nested chains multiply the dry copy, but that is expected. No major efficiency issue.

### 2.4 Verdict

The builtin devices are functionally simple and should be fast enough for a small project, but `Tool` and `EQ` leave obvious CPU on the table. Add criterion benchmarks:

- one Tool per track for 64 tracks at 48 kHz;
- one 4-band EQ per track for 64 tracks;
- a chain with nested Tools/EQs;
- 512-frame blocks, measure real-time factor and worst-case block time.

Target a comfortable margin below the audio deadline, not just "it runs".

---

## 3. Audio Import

### 3.1 Current call path

Both menu import and drag-and-drop run synchronously on the UI thread:

```text
gui::apply_dialog(DialogAction::ImportAudio)        gui.rs:741-748
  -> Workspace::import_audio                        workspace.rs:997-1019
       -> notwig_project::import_asset              assets.rs:212-261
            -> decode_audio / decode_symphonia      assets.rs:20-119
            -> resample (High)                      assets.rs:121-186
            -> duplicate_mono / clone samples       assets.rs:230-234, 264-271
            -> write_wav_f32                         assets.rs:188-209, 243
            -> hash_file (reads entire WAV)          assets.rs:244, 273-278
       -> compute_peaks on the full buffer          workspace.rs:1010-1011
       -> RegisterAsset command                     workspace.rs:1013-1016
            -> structural engine rebuild request    workspace.rs:256-298
```

Drag-and-drop follows the same path through `perform_drop` -> `import_audio_at` (`gui.rs:760-788`).

### 3.2 Why a long MP3 hangs the app

It is not one bug; it is a pipeline that does too much work on the UI thread and does some of it inefficiently:

1. **Full decode.** Symphonia decodes the entire MP3 into one interleaved `Vec<f32>` (`assets.rs:36-119`). This is CPU-bound and proportional to duration.
2. **Per-frame allocation in resampling.** The high-quality resampler allocates a `Vec` for the filter weights inside the output-frame loop (`assets.rs:145-160`):

   ```rust
   for out_frame in 0..out_frames {
       let position = out_frame as f64 / ratio;
       let mut weights = Vec::with_capacity((radius * 2 + 1) as usize);
       ...
   }
   ```

   For a 5-minute 44.1 kHz source resampled to 48 kHz, `out_frames` is about 14.4 million. That is roughly **14.4 million heap allocations**, each tiny, plus per-tap math per channel. This alone can take seconds and fragment/heat the allocator.
3. **Extra full-buffer copies.** `resample` returns `input.clone()` when rates match (`assets.rs:128-131`). `import_asset` then clones again for stereo (`assets.rs:230-234`). Mono is duplicated. The decoded buffer, resampled buffer, and final buffer can all coexist.
4. **Large WAV write and re-read.** Everything is written to a new WAV at project rate (`assets.rs:242-247`, which for a long MP3 is much larger than the source), then `hash_file` reads the entire WAV back into memory and SHA-256s it (`assets.rs:273-278`).
5. **Peak scan.** `compute_peaks` walks every frame again on the UI thread (`workspace.rs:1010-1011, 1470-1488`).
6. **Structural rebuild.** `RegisterAsset` is marked structural (`crates/notwig-core/src/protocol.rs` `requires_plan_rebuild`), so each import requests a full engine rebuild. The build itself is now on a worker (`workspace.rs:256-298`), so it no longer freezes the UI, but it reloads all plugins and competes for CPU/disk during the import. While a build is in flight, requests coalesce into one queued rebuild (`engine_rebuild_queued`), but a batch of imports can still cause multiple rebuild cycles and unnecessary plugin reloads.
7. **Opening a project re-does asset work.** `load_assets` decodes every stored WAV and recomputes peaks on the UI thread (`workspace.rs:172-193`), even though the bundle has a `peaks/` directory that is not used.

### 3.3 Should import be offloaded to a thread?

**Yes. This is necessary, not optional.** The UI must not decode/resample/hash/peak-scan user files on the event loop. But offloading alone is not enough: the per-frame allocation and extra copies make the background job unnecessarily slow and memory-hungry.

Proposed design:

```text
UI: user chooses file
  -> start_import(path, bundle, project_rate) -> Receiver<ImportProgress/Result>
Worker:
  probe/decode (streaming where possible)
  resample with fixed-size weight kernels (no per-frame allocation)
  write WAV (or chosen storage format) in bounded chunks
  hash while writing (streaming SHA-256)
  compute peaks during the same pass
  send ImportedAsset { entry, buffer } back
UI:
  on completion, apply RegisterAsset and create the clip
  show progress/cancel; never mutate the document from the worker
```

Additional optimizations:

- Replace the `Vec` weights with a fixed array such as `[f64; 7]` for radius 3, or precompute a polyphase table once per resample.
- Use `resample` only when rates differ (already done); avoid the extra clone when they match.
- Avoid `converted.samples.clone()` and `duplicate_mono` copies by consuming/moving buffers and writing directly from the decoded buffer. Interleave/split once.
- Hash with a streaming writer (`Sha256::update` from a `BufWriter` shim) instead of reading the finished file.
- Compute peaks in the same decode/resample pass.
- Cache peaks in the bundle `peaks/` directory keyed by asset hash and sample rate, and skip recompute on open.
- Lazy-load or background-load assets on project open; send `EngineMsg::PrepareAudioAsset` as each asset becomes ready.
- Consider storing imported audio as FLAC or preserving the source when it is already compressed, to avoid the large WAV expansion. If the engine needs a normalized buffer, decode on load in the background. This is a storage policy decision; document it.
- Coalesce `RegisterAsset` rebuilds: add the asset to the document and update the engine's asset table without reloading plugins. `EngineMsg::PrepareAudioAsset` already exists (`crates/notwig-engine/src/engine.rs:192-201`), so today's rebuild is unnecessary for an asset-only change.

### 3.4 Import tests and budgets

Add tests with a generated long file, for example 60 seconds of stereo 44.1 kHz:

- import completes and the resulting asset has the expected frame count and duration;
- import does not block the UI thread (structure the worker API so this is testable);
- resampling a known tone preserves length within one frame and does not allocate per frame (use an allocation-counting test helper if practical);
- peaks match a reference computation;
- importing two files can be cancelled cleanly and leaves no temporary files.

Avoid using a real 10-minute MP3 in CI; generate a WAV or a short fixture and scale the logic test.

---

## Supported Formats: WAV, MP3, FLAC, OGG

### Current state

`decode_audio` already routes these extensions to Symphonia (`crates/notwig-project/src/assets.rs:20-34`):

```text
wav, wave, aif, aiff, aifc, mp3, flac, ogg, oga
```

The workspace dependency enables the corresponding Symphonia features (`Cargo.toml:45`): `mp3`, `flac`, `ogg`, `vorbis`, `pcm`, `wav`, `aiff`, and `opt-simd`. The import picker also lists them (`crates/notwig-app/src/gui.rs:889-900`).

So FLAC and Ogg Vorbis are **already supported in principle**. What is missing is confidence and polish:

- There are no FLAC/OGG fixtures or tests; only `tone.mp3` and generated WAV/AIFF coverage exists (`crates/notwig-project/tests/bundle.rs:149-200`).
- Format detection is extension-based only. A file with the wrong or missing extension fails even if the content is valid. Use Symphonia probing as the primary path and the extension only as a hint.
- Ogg Opus is not enabled by the current feature set; only Ogg Vorbis should be claimed as supported. Do not advertise Opus until a decoder is added and tested.
- Channel handling is limited: more than two channels are rejected, mono is duplicated to stereo (`assets.rs:219-234`). Document that multichannel files are not downmixed.
- All imported audio is normalized to a project-rate WAV; the original format is not preserved. That is fine for v0 but means importing a long FLAC/OGG consumes much more disk than the source.
- There is no metadata/loop-point import (BWF, iXML, Vorbis comments). Not required for v0, but state it explicitly.

### Notes for adding/validating FLAC and OGG

- Add small fixtures (`tone.flac`, `tone.ogg`) and tests for sample rate, channels, duration, and a known sine amplitude.
- Add an unsupported/corrupt file test to assert a graceful error.
- Prefer content probing: attempt Symphonia probe first, then use the extension to choose a filename filter or hint. Keep the extension check as a fast rejection only if desired.
- If Ogg Opus is wanted, add the corresponding Symphonia codec feature (verify the exact feature name for the pinned 0.5.4 version) and a fixture; do not assume it works from the `ogg` container feature alone.
- If AIFF is already supported, include it in the same fixture matrix so the format matrix is explicit.
- In the UI, either show a single "Audio" filter (current behavior) or group by format; keep the list synchronized with `decode_audio`.

---

## Export Improvements

### Current state

`UiIntent::RenderMixdown` immediately opens a save dialog for a `.wav` file (`crates/notwig-app/src/gui.rs:626-634`, `crates/notwig-ui/src/shell.rs:708-711`). `Workspace::render_to` then:

- starts at sample 0,
- ends at the maximum clip end,
- builds an engine synchronously on the UI thread (`workspace.rs:1438-1466`),
- renders the whole range into one `Vec<f32>` via `render_offline`,
- writes a float WAV with `write_wav_f32`.

There is no start/stop selection, no format choice, no progress, no cancellation, and no streaming. A long render blocks the UI and uses memory proportional to the entire mix.

### Export dialog UX

Add an export dialog (modal) with:

- **Format**: WAV, MP3, OGG Vorbis.
- **Range**: Start and Stop fields in bars:beats:sixteenths (or seconds with a unit toggle), defaulting to:
  - the loop range when loop is enabled, otherwise
  - 0 to the last clip end (or project end if longer).
- **Copy loop start/end**: a button next to the range fields that copies `doc.transport.loop_range.start` and `.end` into Start/Stop. Disable it with a tooltip when the loop is disabled or no loop range exists; alternatively allow copying a stored loop range even when disabled, but make the behavior explicit.
- **Quality options** (format-dependent):
  - WAV: 16-bit / 24-bit / 32-bit float, dither setting (at least document the default).
  - MP3: bitrate (e.g. 128/192/256/320 kbps) and constant/variable quality.
  - OGG: quality/sample-rate metadata.
- **Sample rate**: fixed to the project rate in v0, displayed read-only. Resampling on export is a later feature.
- **Path**: choose after format is selected; set the extension automatically and confirm overwrite.
- **Render / Cancel**, with a progress bar and elapsed/remaining time. Keep the dialog responsive.
- Remember the last used format and quality in app config (not the project bundle), while range defaults come from the project/loop.

Suggested types (keep UI view-only and app owns the action):

```rust
pub enum ExportFormat { Wav(WavOptions), Mp3(Mp3Options), Ogg(OggOptions) }
pub struct ExportRequest {
    pub path: PathBuf,
    pub format: ExportFormat,
    pub start: TimeMus,
    pub end: TimeMus,
}
```

Add `UiIntent::OpenExportDialog` (replacing or supplementing `RenderMixdown`) and have the app manage the dialog state and submit an `ExportRequest`. Do not put file I/O in `notwig-ui`.

### Export backend

- Add `Workspace::start_export(request) -> Receiver<ExportProgress>` (or an app-level job) that runs on a worker.
- Build the engine on the worker, load plugins there (respecting main-thread constraints), and render in bounded blocks.
- Add a streaming offline render API to `notwig-engine`, for example:

  ```rust
  pub fn render_offline_chunked(
      &mut self,
      start_sample: u64,
      end_sample: u64,
      block: usize,
      mut sink: impl FnMut(&[f32]) -> Result<(), EngineError>,
  ) -> Result<(), EngineError>;
  ```

  This avoids holding the entire mix in memory and lets the encoder consume block by block.
- Convert `TimeMus` start/end to samples with the project tempo map and handle rounding the same way as playback.
- Apply plugin state before rendering (see 1.9), otherwise exported audio will not match what the user hears.
- Include a short tail (or an option) for reverb/delay tails; default to the requested range only and document it.
- Normalize/limit policy: do not add a limiter silently. If peak clipping is possible, warn or offer optional normalization in the dialog.

### Encoder choices

`hound` already handles WAV. Symphonia is decode-only, so MP3/OGG export needs encoders:

- **MP3**: a binding to LAME (`mp3lame-encoder`/`lame-sys`) or a pure-Rust encoder such as `shine`. Verify licenses (LAME is LGPL) and distribution implications before adopting.
- **OGG Vorbis**: `vorbis_rs` or a `libvorbis` binding (`ogg` + `vorbis-encoder`). Verify the crate's license and maintenance status.
- Keep encoding behind a small `AudioEncoder` trait in `notwig-project` so `notwig-app` selects format and `notwig-project` owns the format details:

  ```rust
  trait AudioEncoder {
      fn write(&mut self, interleaved: &[f32]) -> Result<(), ProjectError>;
      fn finish(self: Box<Self>) -> Result<(), ProjectError>;
  }
  ```

- Add metadata where cheap (ID3 for MP3, Vorbis comments for OGG); not required for the first version, but reserve the fields.

### Export tests

- Render a known sine and verify WAV sample count for `[start, stop)`.
- Render the same range twice and assert bit-identical WAV output.
- Encode a short snippet to MP3 and OGG, decode with Symphonia, and assert duration/channels (lossy amplitude tolerance).
- Test the "copy loop start/end" action: loop range -> fields -> exact sample range.
- Test cancel: no partial final file or clean removal of the temporary file.

---

## Suggested Implementation Order

1. **Stability first**
   - Guard `Engine::process` against `frames > max_block_size` and validate output length.
   - Send note-offs on Stop; fix the note-off boundary.
   - Add denormal/NaN guards and track gain ramps.
   - Move old-engine drop off the mutex.
2. **Plugin hang protection**
   - Remove plugin metadata probing from the UI thread.
   - Add per-plugin load/activate budgets and a visible failure state.
   - Cache parameter metadata from the scanner so open/insert do not need a probe instance.
3. **Import**
   - Fix `resample` allocations and extra copies; stream hash/peaks.
   - Move the whole import job to a worker with progress/cancel.
   - Make `RegisterAsset` non-structural and feed `PrepareAudioAsset` directly.
   - Cache peaks and background/lazy-load assets on open.
4. **Export**
   - Add the dialog with format/range and the loop-copy button.
   - Add streaming offline render and a worker-based export job.
   - Add WAV options first, then MP3 and OGG encoders.
5. **Stock DSP**
   - Optimize Tool pan and EQ coefficient updates.
   - Add device benchmarks.
6. **Format coverage**
   - Add FLAC/OGG fixtures/tests, content probing, and keep the picker list synchronized.

---

## Validation

- **Block-size stress test**: run the engine at 32, 64, 256, 512, 1024, and 2048 frames; assert no panic and correct output length.
- **Plugin timeout test**: a fake factory/host that sleeps or blocks in `load`/`prepare`; assert the UI reports failure and the rest of the graph still builds.
- **Note lifecycle test**: start a note, stop transport, assert a note-off is delivered to the instrument wrapper.
- **RT allocation smoke test**: run the render path under an allocation counter; assert no allocations in steady state after warm-up.
- **Import benchmark**: generate a 60-second 44.1 kHz stereo source, measure decode/resample/write/peak time, and compare before/after. Target no per-frame allocations.
- **Import UI test**: start an import and assert the event loop continues to repaint (worker-based API).
- **Export range test**: render `[start, stop)` and verify the exact sample count and loop-copy mapping.
- **Stock DSP benchmark**: 64 tracks each with a Tool, then each with a 4-band EQ, at 48 kHz/512 frames; report real-time factor and worst block time.

---

## Primary Code References

| Concern | Location |
|---|---|
| Engine process / loop / offline render | `crates/notwig-engine/src/engine.rs:256-405` |
| Track buffer sizing | `crates/notwig-engine/src/runtime.rs:17-40` |
| Block-time allocations and routing | `crates/notwig-engine/src/runtime.rs:236-241`, `280-350`, `467-558` |
| Note collection / all-notes-off | `crates/notwig-engine/src/runtime.rs:158-166`, `353-448`; `engine.rs:271-276` |
| Bypass / processor build / latency | `crates/notwig-engine/src/runtime.rs:709-741`; `crates/notwig-dsp/src/graph.rs:159-229` |
| Audio callback / mutex / callback allocations | `crates/notwig-app/src/audio.rs:109-163`, `180-205` |
| Engine build worker | `crates/notwig-app/src/workspace.rs:256-355` |
| Plugin metadata probe on UI thread | `crates/notwig-app/src/workspace.rs:867-952` |
| Plugin load / process | `crates/notwig-engine/src/factory.rs:81-91`; `crates/notwig-plugins/src/rack.rs:55-175`, `277-297` |
| Builtin Tool | `crates/notwig-dsp/src/stock.rs:339-403` |
| Builtin EQ | `crates/notwig-dsp/src/stock.rs:589-729` |
| Builtin Chain | `crates/notwig-dsp/src/stock.rs:731-852` |
| Import pipeline | `crates/notwig-project/src/assets.rs:20-278`; `crates/notwig-app/src/workspace.rs:997-1019` |
| Resampler per-frame allocation | `crates/notwig-project/src/assets.rs:145-160` |
| Asset load on open / peaks | `crates/notwig-app/src/workspace.rs:172-193`, `1470-1488` |
| Export (current) | `crates/notwig-app/src/workspace.rs:1438-1466`; `crates/notwig-app/src/gui.rs:626-634`, `728-740` |
| Format list / Symphonia features | `crates/notwig-project/src/assets.rs:20-34`; `Cargo.toml:45` |
| Export UI entry point | `crates/notwig-ui/src/shell.rs:704-711`; `crates/notwig-app/src/gui.rs:626-634` |
