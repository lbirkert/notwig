# Audio import and analysis performance

Status: implemented. The sinc resampler now uses `rubato`, import hashes the
encoded WAV bytes before writing instead of reading the file back, and the dev
profile optimizes the audio-heavy dependencies so debug imports stay usable.

Related: [`Code Structure`](01_code_structure.md),
[`Audio Engine`](02_audio_engine.md), `crates/notwig-project/README.md`, and
`crates/notwig-app/src/workspace/assets.rs`.

## 1. What "audio analysis" means in Notwig today

There are three separate paths, and only one of them is a real CPU cost:

1. **Duration probe.** `notwig_project::probe_audio` reads headers only. It
   parses WAV/AIFF/FLAC/MP3/Ogg metadata without decoding. Measured at roughly
   `0.01-0.02 ms` per file. The arranger drag preview and the import analysis
   worker use it first, falling back to a full decode only if probing fails.
2. **Full import.** `notwig_project::import_asset` decodes the whole source,
   resamples it to the project rate with `rubato`, duplicates mono to stereo,
   encodes a managed WAV in memory, hashes the final bytes, writes the file, and
   returns the PCM buffer. `Workspace` then builds the signed min/max waveform
   pyramid and the cached mean levels used by close rendering (see
   [`Waveform Rendering`](10_waveform_rendering.md)).
3. **Project load.** `Workspace::load_assets` decodes every stored WAV asset and
   rebuilds its waveform pyramids synchronously while opening the project.

There is no spectral analysis, loudness analysis, transient detection, or GPU
work today. The visible "Analyzing audio..." state is the import pipeline.

## 2. Current import pipeline

For a dropped or imported file:

1. `probe_audio` reads the header (fast).
2. A single background `audio_import` thread calls `import_asset`.
3. `decode_audio` reads the entire source file into memory, then Symphonia
   probes it, decodes the default track, and appends interleaved `f32` samples.
4. `resample` converts to the project sample rate with `rubato`'s synchronous
   `Fft` resampler. It prepends two chunks of silence and discards the matching
   output so the first FFT overlap window warms up outside the audible signal;
   without that run-in the first ~20 ms of high-frequency material is corrupted.
   If the rates already match, it clones the whole buffer.
5. Mono is duplicated to stereo.
6. `write_wav_f32_bytes` encodes a 32-bit float WAV in memory.
7. `hash_bytes` computes SHA-256 over those final WAV bytes. The bytes are
   written to the asset-unique temp path and renamed to `<hash>.wav`.
8. On the UI thread, `complete_audio_import` builds the waveform pyramids and
   applies document commands.

Multiple files share one import thread, so imports are serialized. Project open
does its decoding and waveform work synchronously on the loading thread.

The bundle already reserves a `peaks/` directory, but nothing writes to or reads
from it. Waveform pyramids and mean caches are rebuilt from PCM every time.

## 3. Measurements

Method: a temporary release-mode harness outside the repository, `lto = "thin"`,
`codegen-units = 1`, on an Apple M1 Pro (arm64, macOS 15.7.4). Input was a
60-second stereo signal, 44.1 kHz unless noted. Values are best-of-N wall-clock
times; they include single-thread work only.

### Per-stage costs, 60 s stereo 44.1 kHz source

| Stage | Before | After | Notes |
| --- | --- | --- | --- |
| `probe_audio` | 0.01 ms | 0.01 ms | Header only |
| Decode WAV | 13 ms | 13 ms | 32-bit float WAV |
| Decode MP3 | 42 ms | 41 ms | Symphonia |
| Decode FLAC | 38 ms | 39 ms | Symphonia |
| Decode Ogg Vorbis | 36 ms | 36 ms | Symphonia |
| `resample` High (44.1 -> 48) | 228 ms | ~56 ms | `rubato` synchronous FFT |
| `resample` Standard (44.1 -> 48) | 24 ms | ~56 ms | Same `rubato` FFT path |
| Same-rate `resample` | 1.7 ms | 1.8 ms | Buffer clone |
| Waveform pyramids, min/max + mean (replaces flat 2048-bucket peaks) | 11 ms | ~11-15 ms | One linear post-decode pass; mean levels stop at the close-render threshold |
| Encode WAV bytes | - | 8 ms | New in-memory encode |
| SHA-256 over WAV bytes | 63 ms | 63 ms | Still the dominant hash cost |
| Write WAV bytes to disk | 20 ms | 14 ms | Same managed file |
| Read WAV back for hash | ~2-3 ms | - | Removed |

The resampler is now about four times faster for a 60 s 44.1 -> 48 kHz clip.
A polyphase kernel table can go slightly faster (~33 ms in the prototype), but
`rubato` is maintained, SIMD-friendly, and removes the custom trig-heavy loop.

The hashing change removes the read-back pass, but it does **not** remove
SHA-256 itself. On this machine the old "read + SHA-256" measurement was about
66 ms, of which only a few milliseconds were the cached file read; SHA-256 over
the 10.9 MiB WAV is about 63 ms. The import now hashes the exact bytes before
writing, so the extra disk read is gone, but the hash algorithm still dominates
the 48 kHz import path. A faster content hash or avoiding re-hashing unchanged
assets would be needed for a larger win there.

### End-to-end import, 60 s stereo

| Source | Project rate | Before | After |
| --- | --- | --- | --- |
| WAV 44.1 kHz | 48 kHz | 353 ms | 175 ms |
| MP3 44.1 kHz | 48 kHz | 372 ms | 203 ms |
| FLAC 44.1 kHz | 48 kHz | 366 ms | 196 ms |
| Ogg 44.1 kHz | 48 kHz | 366 ms | 189 ms |
| WAV 48 kHz | 48 kHz | 123 ms | 122 ms |

The 44.1 kHz cases improved by roughly 50 percent because resampling was the
dominant cost. At 48 kHz, where resampling is skipped, import is essentially
unchanged: it is already dominated by SHA-256, not decoding or I/O.

### Debug builds

The release numbers above did not match a debug build. Before the profile
change, a debug build took about 5.3 s to import a 60 s 44.1 -> 48 kHz WAV and
about 1.45 s for a same-rate 44.1 kHz WAV. A three-minute same-rate WAV therefore
landed around 4.4 s, plus roughly 1 s of waveform work, which explains a six-second
"Analyzing audio..." pause.

`Cargo.toml` now optimizes dependencies and `notwig-project` in the dev profile:

```toml
[profile.dev.package."*"]
opt-level = 2

[profile.dev.package.notwig-project]
opt-level = 2
```

The wildcard keeps Symphonia, rubato, realfft, sha2, and the rest optimized
without optimizing every workspace crate. The explicit `notwig-project` entry
matters because rubato's generic `process_all` path is instantiated inside that
crate; without it, the dependency is optimized but the monomorphized hot path
still runs at `opt-level = 0`.

Measured in the actual workspace debug profile:

| Debug import, 60 s 44.1 kHz WAV | Before | After |
| --- | --- | --- |
| Same-rate 44.1 kHz | 1447 ms | 143 ms |
| Resampled to 48 kHz | 5275 ms | 270 ms |

A three-minute import should now be roughly 0.8-1.2 s including waveform
computation, instead of about 5-16 s depending on the source rate. Incremental
rebuilds of `notwig-project` remain fast (about 1.7 s in this workspace) because
the rest of the workspace stays at `opt-level = 0`. The first build after a
clean checkout pays a one-time cost to compile optimized dependencies.

Normalize per minute of stereo audio: decoding is 13-42 ms, `rubato` resampling
is about 56 ms, the waveform pyramids are about 11-15 ms, and encoding plus hashing
plus writing is about 85 ms. Opening a project still repeats decode plus waveform
work for every asset, so a
project with ten five-minute assets pays roughly 1.3 s of avoidable synchronous
work on open even before any resampling.

## 4. Why Bitwig is faster (and why it is probably not GPU decoding)

The large gap is more likely architectural than a missing GPU decoder:

- Notwig eagerly decodes **and** resamples **and** transcodes to WAV on import.
  A DAW can instead keep the original compressed file, build a waveform cache,
  and defer or stream decoding.
- Notwig rebuilds waveform pyramids on every open. A persistent waveform cache
  is the standard DAW solution and is already implied by the bundle layout.
- Notwig imports one file at a time on a single worker thread.
- The old custom resampler computed two `sin` calls per tap per output frame.
  That was an algorithm problem, not a hardware problem, and `rubato` now
  replaces it.

Symphonia's decoders are already fast enough: MP3 decodes a minute of stereo in
about 41 ms, roughly 1400x real time. Replacing or GPU-accelerating the decoder
cannot recover the time previously spent in resampling, and even the optimized
import is now dominated by SHA-256 and waveform work rather than decoding.

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
- a table-based CPU resampler prototype turned 228 ms into about 33 ms for this
  file, while the adopted `rubato` path lands at about 56 ms.

### Duration analysis and header probing

The import worker needs a duration for the drag preview and placement. For WAV,
AIFF, FLAC, MP3, and Ogg that value is encoded in the header, so `probe_audio`
reads a few kilobytes and returns in about `0.01-0.02 ms` per file. Full decode
is only a fallback when a header cannot be parsed. A GPU path would first upload
the file and pay shader setup costs to parse one small header, with no
parallel work to hide them. Where analysis does become measurable (waveform
min/max reduction or resampling), the data-parallel work is a better fit for
SIMD first and GPU second, because the CPU path avoids upload, download, and
shader-compilation overhead.

## 6. Alternatives, ranked by expected impact

| Option | Status / expected impact | Cost and risk |
| --- | --- | --- |
| Adopt `rubato` for resampling | **Implemented.** 228 -> ~56 ms per 60 s 44.1 -> 48 kHz clip | Done; output tests added |
| Avoid write-then-read-back hashing | **Implemented.** Removes the extra disk read; SHA-256 itself remains | Done; filename is still SHA-256 of stored bytes |
| Precompute a custom sinc kernel table | Prototype was ~33 ms for the same clip, slightly faster than `rubato` | Not needed unless `rubato` overhead becomes material |
| Use a faster content hash (for example BLAKE3) | Would attack the remaining ~63 ms SHA-256 cost at 48 kHz | Medium: changes asset naming/hash format |
| Persist waveform pyramids under `peaks/` keyed by asset hash | Removes about 11 ms per minute per asset on open and re-import | Medium: needs a versioned waveform format and invalidation |
| Parallelize imports across files | Near-linear for multi-file drops on multiple cores | Medium: worker pool, ordering, error reporting, memory pressure |
| Stream decode from `File`, preallocate output, reuse `SampleBuffer` | Modest decode improvement; avoids whole-file memory copy and per-packet allocations | Low-medium; touches the Symphonia loop |
| Use a WAV fast path (`hound` or direct PCM conversion) | Avoids general-purpose codec overhead for the common case | Low; keeps Symphonia for compressed formats |
| macOS `AudioToolbox`/`ExtAudioFile` or `AVFoundation` adapter | Potentially faster decode and OS-optimised sample-rate conversion; hardware assistance varies | Medium-high: platform adapter, format differences, error mapping |
| Accelerate/vDSP for waveform analysis and resampling | SIMD/NEON min/max and convolution speedups without GPU transfer | Medium: platform-specific or a portable SIMD abstraction |
| `wgpu` compute for resampling/waveforms/FFT | Could help large batch or spectral workloads | High: shader code, cold start, quality and determinism testing |
| GPU audio decoding (`wgpu`, CUDA, Metal) | Little to no gain for one-file imports | High and unproven; no mature Rust ecosystem |

Useful crates to evaluate: [`rubato`](https://crates.io/crates/rubato) for
resampling, [`hound`](https://crates.io/crates/hound) for WAV,
[`claxon`](https://crates.io/crates/claxon) for FLAC,
[`lewton`](https://crates.io/crates/lewton) for Vorbis, and `minimp3` bindings
for MP3 if Symphonia decode ever becomes a bottleneck. For GPU spectral work,
[`wgsl-fft`](https://github.com/larsjoost/wgsl-fft) and
[`gpu-fft`](https://github.com/eugenehp/gpu-fft) exist, but neither is needed for
the current waveform renderer.

## 7. Recommendation

The first two CPU items are done. The remaining work, in order:

1. Persist and version waveform pyramids in `peaks/`; load them on open instead of decoding
   every asset synchronously.
2. Decide whether the remaining SHA-256 cost is acceptable. If not, evaluate a
   faster content hash such as BLAKE3 (this changes asset naming semantics and
   needs a migration story).
3. Decode from a `File`, reserve output capacity, and reuse the packet sample
   buffer.
4. Add a small worker pool for concurrent imports.
5. Only then consider a platform decoder adapter or a `wgpu` compute path for
   resampling or FFT, and benchmark it against the optimized CPU path on real
   files.

A GPU decoder is not the right first investment. Decoding remains 4-12 percent
of a 44.1 kHz import; resampling is now roughly 56 ms and the managed-asset hash
plus write is roughly 85 ms. The next meaningful wins are waveform caching,
hashing, and multi-file concurrency, not GPU decode.
