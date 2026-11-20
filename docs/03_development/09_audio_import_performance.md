# Audio import and analysis performance

Status: analysis. No implementation changes in this pass.

Related: [`Code Structure`](01_code_structure.md),
[`Audio Engine`](02_audio_engine.md), `crates/notwig-project/README.md`, and
`crates/notwig-app/src/workspace/assets.rs`.

## 1. What "audio analysis" means in Notwig today

There are three separate paths, and only one of them is a real CPU cost:

1. **Duration probe.** `notwig_project::probe_audio` reads headers only. It
   parses WAV/AIFF/FLAC/MP3/Ogg metadata without decoding. Measured at roughly
   `0.01-0.02 ms` per file. The arranger drag preview uses it first.
2. **Full import.** `notwig_project::import_asset` decodes the whole source,
   resamples it to the project rate, duplicates mono to stereo, writes a managed
   WAV, hashes that WAV, and returns the PCM buffer. `Workspace` then computes a
   2048-bucket peak waveform.
3. **Project load.** `Workspace::load_assets` decodes every stored WAV asset and
   recomputes its peaks synchronously while opening the project.

There is no spectral analysis, loudness analysis, transient detection, or GPU
work today. The visible "Analyzing audio..." state is the import pipeline.

## 2. Current import pipeline

For a dropped or imported file:

1. `probe_audio` reads the header (fast).
2. A single background `audio_import` thread calls `import_asset`.
3. `decode_audio` reads the entire source file into memory, then Symphonia
   probes it, decodes the default track, and appends interleaved `f32` samples.
4. `resample` converts to the project sample rate. If the rates already match,
   it clones the whole buffer.
5. Mono is duplicated to stereo.
6. `write_wav_f32` writes a 32-bit float WAV into `assets/`.
7. `hash_file` reads the new WAV back into memory and computes SHA-256 for its
   filename.
8. On the UI thread, `complete_audio_import` calls `compute_peaks` and applies
   document commands.

Multiple files share one import thread, so imports are serialized. Project open
does its decoding and peak work synchronously on the loading thread.

The bundle already reserves a `peaks/` directory, but nothing writes to or reads
from it. Peaks are rebuilt from PCM every time.

## 3. Measurements

Method: a temporary release-mode harness outside the repository, `lto = "thin"`,
`codegen-units = 1`, on an Apple M1 Pro (arm64, macOS 15.7.4). Input was a
60-second stereo signal, 44.1 kHz unless noted. Values are best-of-N wall-clock
times; they include single-thread work only.

### Per-stage costs, 60 s stereo 44.1 kHz source

| Stage | Time | Notes |
| --- | --- | --- |
| `probe_audio` | 0.01 ms | Header only |
| Decode WAV | 13 ms | 32-bit float WAV |
| Decode MP3 | 42 ms | Symphonia |
| Decode FLAC | 38 ms | Symphonia |
| Decode Ogg Vorbis | 36 ms | Symphonia |
| `resample` High (44.1 -> 48) | 228 ms | 7-tap windowed sinc, per-tap `sin` |
| `resample` Standard (44.1 -> 48) | 24 ms | 3-tap linear |
| `resample` High with precomputed kernel table | 32.5 ms | Same 7-tap kernel, no per-sample trig |
| Same-rate `resample` | 1.7 ms | Buffer clone |
| `compute_peaks` (2048 buckets) | 11 ms | Post-decode pass |
| `write_wav_f32` | 20 ms | Managed WAV write |
| Read back + SHA-256 | 65 ms | Hash for the managed asset filename |

### End-to-end import, 60 s stereo

| Source | Project rate | Observed import |
| --- | --- | --- |
| WAV 44.1 kHz | 48 kHz | 353 ms |
| MP3 44.1 kHz | 48 kHz | 372 ms |
| FLAC 44.1 kHz | 48 kHz | 366 ms |
| Ogg 44.1 kHz | 48 kHz | 366 ms |
| WAV 48 kHz | 48 kHz | 123 ms |

These figures line up with the stage breakdown: 44.1 kHz imports spend roughly
two-thirds of their time in the sinc resampler, then another fifth reading and
hashing the WAV they just wrote. Decoding is a small fraction. At 48 kHz, where
resampling is skipped, import is dominated by write and hash, not decoding.

Normalize per minute of stereo audio: decoding is 13-42 ms, resampling is
228 ms with the current High path, peaks are 11 ms, and write plus hash is about
85 ms. Opening a project repeats decode plus peaks for every asset, so a project
with ten five-minute assets pays roughly 1.3 s of avoidable synchronous work on
open even before any resampling.

## 4. Why Bitwig is faster (and why it is probably not GPU decoding)

The large gap is more likely architectural than a missing GPU decoder:

- Notwig eagerly decodes **and** resamples **and** transcodes to WAV **and**
  re-reads that WAV for a hash on import. A DAW can instead keep the original
  compressed file, build a peak cache, and defer or stream decoding.
- Notwig rebuilds peaks on every open. A persistent peak cache is the standard
  DAW solution and is already implied by the bundle layout.
- Notwig imports one file at a time on a single worker thread.
- Notwig's High resampler contains two `sin` calls per tap per output frame. That
  is an algorithm problem, not a hardware problem.

Symphonia's decoders are already fast enough: MP3 decodes a minute of stereo in
about 42 ms, roughly 1400x real time. Replacing or GPU-accelerating the decoder
cannot recover the 228 ms spent resampling.

## 5. GPU decoding reality check

Decoding is the wrong first target for the GPU:

- **Audio codecs are mostly serial.** MP3 uses variable-length Huffman decode, a
  bit reservoir, and overlap-add with state carried between frames. FLAC and
  Vorbis have similar frame dependencies. A single file exposes little
  fine-grained parallelism, so GPU lanes sit idle and kernel/transfer overhead
  dominates.
- **Hardware video decoders do not generally decode audio.** NVDEC, VideoToolbox,
  and VA-API are video paths. There are research GPU FLAC decoders and patents
  for GPU audio decode, but there is no mainstream, cross-platform Rust crate or
  stable OS API that matches `symphonia`'s coverage.
- **The practical GPU parallelism for decoding is across files.** If hundreds of
  files must be analysed, batching one file per GPU queue slot can make sense.
  Importing a handful of files does not.
- **The expensive stages are better GPU candidates than decode.** Resampling,
  peak reduction, normalization, FFT, and spectrograms are data-parallel. Even
  there, the first question should be "can SIMD or a better algorithm remove the
  cost?" because the CPU options are simpler and have no upload/download or
  shader-compilation cost.

The published GPGPU audio work points the same way: transfer and launch overhead
matter, and the wins come from large, parallel, batchable workloads rather than
small sequential codec loops. See the
[DAFx 2024 GPGPU audio benchmark framework](https://www.dafx.de/paper-archive/2024/papers/DAFx24_paper_56.pdf),
the [NIME 2020 practicality study](https://muses-dmi.github.io/assets/There_and_back_again_Evaluating_the_practicality_of_GPU_accelerated_digital_audio_nime20.pdf),
and the [FLAC GPU decoding paper](https://ieeexplore.ieee.org/document/8672328)
for the batch-oriented and codec-specific exceptions.

On Apple Silicon, unified memory removes the classic PCIe copy penalty, and the
app already links `wgpu` through `eframe`. A Metal compute path is therefore
technically available. It is still not the shortest route to a faster import:

- a compute resampler needs arbitrary-ratio polyphase kernels, edge handling,
  and a deterministic output contract;
- the first dispatch pays shader compilation cost;
- decoding remains on the CPU, so Amdahl's law caps the gain;
- a table-based CPU resampler already turns 228 ms into 32.5 ms for this file.

## 6. Alternatives, ranked by expected impact

| Option | Expected impact | Cost and risk |
| --- | --- | --- |
| Precompute the sinc kernel table (or adopt `rubato`) | Removes ~195 ms per 60 s 44.1 -> 48 kHz import (about 7x on the resampler) | Low risk; keep the same kernel shape and test output |
| Avoid write-then-read-back hashing | Removes most of the 65 ms hash pass; hash while writing or hash the source stream | Low risk; adjust asset naming and atomic-write flow |
| Persist peaks under `peaks/` keyed by asset hash | Removes 11 ms per minute per asset on open and re-import | Medium: needs a versioned peak format and invalidation |
| Parallelize imports across files | Near-linear for multi-file drops on multiple cores | Medium: worker pool, ordering, error reporting, memory pressure |
| Stream decode from `File`, preallocate output, reuse `SampleBuffer` | Modest decode improvement; avoids whole-file memory copy and per-packet allocations | Low-medium; touches the Symphonia loop |
| Use a WAV fast path (`hound` or direct PCM conversion) | Avoids general-purpose codec overhead for the common case | Low; keeps Symphonia for compressed formats |
| macOS `AudioToolbox`/`ExtAudioFile` or `AVFoundation` adapter | Potentially faster decode and OS-optimised sample-rate conversion; hardware assistance varies | Medium-high: platform adapter, format differences, error mapping |
| Accelerate/vDSP for peaks and resampling | SIMD/NEON peak and convolution speedups without GPU transfer | Medium: platform-specific or a portable SIMD abstraction |
| `rubato` pure-Rust resampler | SIMD-friendly sinc/FFT resampling, chunked API | Low-medium: new dependency, output/quality tests |
| `wgpu` compute for resampling/peaks/FFT | Could help large batch or spectral workloads | High: shader code, cold start, quality and determinism testing |
| GPU audio decoding (`wgpu`, CUDA, Metal) | Little to no gain for one-file imports | High and unproven; no mature Rust ecosystem |

Useful crates to evaluate: [`rubato`](https://crates.io/crates/rubato) for
resampling, [`hound`](https://crates.io/crates/hound) for WAV,
[`claxon`](https://crates.io/crates/claxon) for FLAC,
[`lewton`](https://crates.io/crates/lewton) for Vorbis, and `minimp3` bindings
for MP3 if Symphonia decode ever becomes a bottleneck. For GPU spectral work,
[`wgsl-fft`](https://github.com/larsjoost/wgsl-fft) and
[`gpu-fft`](https://github.com/eugenehp/gpu-fft) exist, but neither is needed for
the current peak-only waveform.

## 7. Recommendation

Do the CPU and caching work first. It is measurable, low-risk, and larger than
what a GPU decoder could plausibly deliver:

1. Replace the per-sample-trig resampler with a precomputed polyphase table or
   `rubato`; keep the current quality option names and add output tests.
2. Hash while writing (or hash the original stream) instead of reading the
   managed WAV back.
3. Persist and version peaks in `peaks/`; load them on open instead of decoding
   every asset synchronously.
4. Decode from a `File`, reserve output capacity, and reuse the packet sample
   buffer.
5. Add a small worker pool for concurrent imports.
6. Only then consider a platform decoder adapter or a `wgpu` compute path for
   resampling or FFT, and benchmark it against the optimized CPU path on real
   files.

A GPU decoder is not the right first investment. The current data says decoding
is 4-12 percent of a 44.1 kHz import; resampling and redundant I/O are the
actual costs.
