# DSP implementation and performance

Status: implemented DSP, benchmark harness added.

Related: [`Audio Engine`](02_audio_engine.md),
[`Code Structure`](01_code_structure.md), and
`crates/notwig-core/README.md`.

## 1. What `notwig-core::dsp` does

`notwig-core::dsp` is the boundary between the editable document and the real-time
engine. It does not own the audio callback. It owns four things:

1. **Device contracts.** `DeviceSpec`, ports, signal types, `DeviceProcessor`,
   `ProcessStatus`, note events, and the processing context.
2. **Parameters.** Ranges, normalized mapping, units, flags, and smoothing
   metadata.
3. **Plan compilation.** `compile` turns a validated `ProjectDoc` into a
   `CompiledPlan`: processing order, per-track latency labels, send delay
   requirements, and the device chains to instantiate.
4. **Stock processors.** `Tool`, `EQ`, and `Chain`, plus the EQ magnitude
   response used by the UI.

`notwig-engine` builds processors from the plan, prepares them off the audio
thread, and runs `process` once per block. The callback moves prepared state
into place and never compiles graphs, resolves specs, or allocates.

## 2. The real-time signal path

For each chunk the engine does the following, in order:

1. Clear each track buffer and its pre-fader snapshot.
2. Render audio clips into track buffers.
3. Apply the track input delay used for latency alignment.
4. Collect MIDI note events for the block.
5. Process the track's main device chain in order.
6. Apply track gain and pan.
7. Route the buffer to its parent/master and to any sends, padding each
   connection by its planned latency difference.
8. Update peak/RMS meters.
9. Copy the master buffer to the output.

Stock processing is block-based and interleaved stereo. A block is at most
`RenderConfig::max_block_size` frames; the engine splits larger requests.

## 3. Stock devices

### Tool

A stereo utility processor. Its per-frame work is:

- eight one-pole smoothed values (input gain, output gain, pan, width, dry/wet,
  polarity L/R, and an internal L/R swap value);
- equal-power pan gains computed with `cos`/`sin`;
- polarity inversion, optional L/R swap, mid/side width;
- dry/wet blend and output gain.

Everything runs in `f32`. The smoothed values update once per sample, which
keeps parameter changes click-free. `Tool` has no filter state and cannot
denormal.

`p_swap_lr` is part of `tool_spec`; the smoother crossfades the channels over
roughly 8 ms, so the swap stays click-free.

### EQ

Four bands, each one of Bell, low shelf, high shelf, high pass, or low pass.
Coefficients follow the standard RBJ cookbook formulas in `f64`. Each band is a
direct-form II transposed biquad with two state values per channel.

- A band has a `current` and `target` coefficient set. When parameters change,
  coefficients ramp linearly over 10 ms (`ramp_samples`) to avoid zipper noise.
- An `enabled_mix` smoother fades a band in or out; disabled bands still run
  their biquad and blend the result.
- A global Shift parameter transposes every band's frequency in semitones.
- Parameters are stored in a `BTreeMap<ParamId, f64>`.
- `process` re-derives every band's coefficients at the start of every block,
  whether or not anything changed.

### Chain

A container processor with input gain, output gain, and dry/wet. It copies the
dry signal, runs each child in order, then mixes wet and dry. Its reported
latency is the sum of its children's latency.

### Extension containers

`ContainerProcessor` is the host-owned container for extension devices. It
holds an optional stage (the extension processor) and a serial child chain.

- `StageThenChildren` runs the stage, then the children; `ChildrenThenStage`
  does the reverse.
- With a wet parameter, it copies the dry input first, runs the stage and
  children as the wet path, and crossfades with a 15 ms smoothed mix. The stage
  outputs wet only.
- Nested parameters route into the child chain. Latency is the stage plus all
  children.
- If the stage is missing, the children still run and the wet blend is skipped.

The stock Chain keeps its separate `ChainProcessor`; extension containers do not
change its behavior or performance. `ChainProcessor::process` uses the dry
buffer sized during `prepare`. Blocks larger than that capacity are processed in
chunks without allocating; note events are delivered with the first chunk.

## 4. Benchmark methodology

The benchmark lives in `crates/notwig-core/benches/dsp.rs` and uses Criterion.
Run it with:

```sh
cargo bench -p notwig-core
```

For a faster local pass:

```sh
cargo bench -p notwig-core -- --warm-up-time 0.5 --measurement-time 1.5 --sample-size 20
```

Groups:

| Group | Measures |
| --- | --- |
| `stock/tool/process/{64,256,1024}` | `ToolProcessor::process` at three block sizes |
| `stock/eq/process/{64,256,1024}` | `EqProcessor::process` with four active bands |
| `stock/eq/process-disabled/256` | `EqProcessor::process` with every band disabled |
| `stock/chain/process/{64,256,1024}` | `Chain` containing Tool, EQ, Tool |
| `stock/tool/construct/prepare` | Tool construction and prepare |
| `graph/compile/{1,8,32}` | `compile` for 1, 8, and 32 audio tracks, each with Tool and EQ |
| `graph/eq_magnitude_response/256_points` | 256-point EQ curve evaluation |
| `reference/pan_gains/single` | One equal-power pan gain calculation |

Throughput counts interleaved `f32` samples. A stereo 48 kHz stream consumes
96,000 samples per second, so `X` million samples per second means the
processor could handle roughly `X / 0.096` simultaneous real-time streams on
one core, ignoring routing and metering.

## 5. Current performance

Measured on an Apple M1 Pro (arm64, macOS 15.7.4) with the workspace release
profile (`lto = "thin"`, `codegen-units = 1`), Criterion quick settings.
Treat these as relative indicators, not platform guarantees.

| Benchmark | Block / input | Time | Throughput |
| --- | --- | --- | --- |
| Tool | 64 frames | 628 ns | 204 Melem/s |
| Tool | 256 frames | 2.49 us | 205 Melem/s |
| Tool | 1024 frames | 10.13 us | 202 Melem/s |
| EQ (4 bands) | 64 frames | 1.117 us | 115 Melem/s |
| EQ (4 bands) | 256 frames | 3.72 us | 138 Melem/s |
| EQ (4 bands) | 1024 frames | 13.91 us | 147 Melem/s |
| EQ (all disabled) | 256 frames | 3.50 us | 146 Melem/s |
| Chain (Tool+EQ+Tool) | 64 frames | 2.77 us | 46 Melem/s |
| Chain (Tool+EQ+Tool) | 256 frames | 10.20 us | 50 Melem/s |
| Chain (Tool+EQ+Tool) | 1024 frames | 39.87 us | 51 Melem/s |
| EQ response | 256 points | 71.75 us | 3.57 Melem/s |
| Compile | 1 track | 1.72 us | - |
| Compile | 8 tracks | 12.9 us | - |
| Compile | 32 tracks | 63.6 us | - |
| Pan gains | one frame | 2.40 ns | - |

A 256-frame block at 48 kHz represents 5.33 ms of audio. Tool takes 2.49 us
(about 2100x real time), EQ 3.72 us (about 1400x), and the three-device Chain
10.2 us (about 520x). A project with 32 tracks, each running Tool then EQ, costs
roughly 199 us per 256-frame block, or about 3.7 percent of one core for stock
device math alone.

Compilation is far below audio deadlines even at 32 tracks: about 64 us. The EQ
response calculation costs about 0.28 us per frequency point; a 1000-point UI
curve would be about 280 us, acceptable but worth caching once it becomes a
per-frame cost.

## 6. Where the time goes

- **EQ is the most expensive stock device.** Four bands run for both channels
  at all times. Disabling every band only saves about six percent: coefficients
  are still rebuilt, and the disabled biquads still execute before the mix
  fade. The coefficient rebuild (`powf`, `sin`, `cos`, `sqrt`) and the
  `BTreeMap` parameter lookups happen once per band per block, not per sample,
  but they are pure overhead when nothing changed.
- **Tool's per-sample pan gains are the clearest avoidable cost.** The smoothed
  pan value changes slowly, but `Pan::gains()` is evaluated for every frame.
  One pair of `cos`/`sin` calls costs about 2.4 ns on this machine, roughly a
  quarter of Tool's 9.7 ns per frame.
- **Sample smoothing costs scale with parameter count.** Tool runs eight
  one-pole updates per frame, even after a value has settled. The exponential
  smoother is cheap but not free.
- **Chain overhead is modest.** Three children take about 34 ns per frame; the
  Chain wrapper adds roughly 6 ns per frame for the dry copy, wet/dry mix, and
  gain smoothers.

## 7. Would `dasp` help?

[`dasp`](https://github.com/RustAudio/dasp) is a suite of low-level audio
fundamentals: sample and frame traits, conversions, signal iterators, ring
buffers, interpolation, peak/RMS/envelope detection, windowing, and graph
utilities. It is modular, dependency-free, allocation-free in its core crates,
and `no_std` capable. It is not a filter, mixing, or DAW engine library.

| Need | Covered by `dasp`? | Impact for Notwig |
| --- | --- | --- |
| Biquad/EQ filters | No | Would remain custom or use a filter-specific crate such as `biquad` |
| Gain, pan, width, dry/wet | No | Custom loops are already simple and fast |
| Block mixing and latency compensation | No | Stays in `notwig-engine` |
| Sample/frame conversions | Yes (`dasp_sample`, `dasp_frame`) | Useful only if non-`f32` formats become a real requirement |
| Delay/ring buffers | Yes (`dasp_ring_buffer`) | Could replace `DelayLine`; no meaningful speedup expected |
| Interpolation/resampling | Yes (`dasp_interpolate`) | Relevant later for warping or sample-rate conversion, not today |
| Peak/RMS/envelope metering | Yes (`dasp_peak`, `dasp_rms`, `dasp_envelope`) | Current meter loop is not a bottleneck; adopting changes semantics |
| Signal graphs/iterators | Yes (`dasp_signal`, `dasp_graph`) | Per-sample iterator abstraction is a poor fit for block-based real-time DSP |

The current hot loops are already concrete, allocation-free, and block-based.
`dasp` would add abstraction without removing the expensive work (the biquads,
smoothing, and pan gains). The likely result is neutral to slightly slower code,
plus a dependency whose crates.io release is still `0.11.0` from 2020 even
though the repository receives occasional maintenance commits.

A more relevant library for the expensive part would be a small biquad crate,
but the existing filter is already the standard RBJ design and is only a few
dozen lines. The better return is optimizing the current code first.

**Recommendation:** do not adopt `dasp` wholesale. Revisit it only for a concrete
feature it uniquely serves (for example interpolation when warping is
implemented), and prefer a focused crate over the umbrella dependency when that
happens.

## 8. Current weaknesses

### Performance

1. **EQ coefficient rebuild every block.** `EqProcessor::process` calls
   `update_band` for all four bands unconditionally. Move this to `set_param`,
   or guard it with a dirty flag. This also removes repeated `BTreeMap` lookups
   and `powf` calls.
2. **Disabled EQ bands still cost almost full price.** Track when a band's
   enable mix has settled at zero and skip its biquad once stable.
3. **Per-sample pan trigonometry in Tool.** Compute equal-power gains once per
   block, or recompute at a small interval and interpolate, instead of calling
   `cos`/`sin` per frame.
4. **`BTreeMap` in per-block paths.** EQ parameters are a fixed set; a struct
   with named fields or an array indexed by band is cheaper and clearer.
5. **`f64` EQ state.** It buys numerical headroom but doubles arithmetic and
   state traffic on a `f32` signal path. Measure an `f32` variant before
   deciding; do not change it without listening and stability tests.
6. **No denormal protection.** A biquad fed silence can decay into denormal
   state and become unexpectedly slow on some CPUs. Add a tiny DC offset,
   flush near-zero state, or use a denormal-safe filter form.
7. **Settled smoothers keep working.** Consider block-rate smoothing or a
   cheap settled-state check where profiling shows it matters.

### Correctness and maintenance

1. **Stereo is assumed.** Stock processors use `chunks_exact(2)` and ignore
   `RenderConfig::channels` beyond two. This matches the current product but is
   an implicit contract that should be documented or enforced.
2. **Ramp specs are ignored.** `ParamSet` carries an optional `RampSpec`, but
   `DeviceProcessor::set_param` only receives a value. Ramping behavior is
   hardcoded per device.
3. **No NaN/Inf guard.** A faulty device or plugin can poison downstream
   buffers. Stock devices are safe, but the boundary does not sanitize.
4. **Bypass is structural.** Bypassing a device rebuilds the runtime rather
   than crossfading, so bypass changes can click.
5. **EQ coefficient interpolation has no stability guard.** Linear interpolation
   between stable coefficient sets is common but is not a formal guarantee for
   extreme parameter jumps.

## 9. Suggested order of work

1. Add a dirty flag and move EQ coefficient updates to `set_param`.
2. Hoist Tool pan gains to block rate.
3. Skip settled disabled EQ bands.
4. Replace EQ's `BTreeMap` with fixed storage.
5. Add denormal handling and a sanitization test with silence.
6. Re-run `cargo bench -p notwig-core` and record the new numbers on this page.
7. Evaluate `f32` EQ state and block-rate smoothing only after the structural
   wins, with listening and behavior tests.
