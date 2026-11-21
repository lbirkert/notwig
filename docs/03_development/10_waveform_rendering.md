# Waveform rendering

Status: implemented. Audio clips draw signed, level-of-detail waveforms and can
zoom down to individual samples.

Related: [`Code Structure`](01_code_structure.md),
[`Audio Import and Analysis Performance`](09_audio_import_performance.md), and
`crates/notwig-ui/src/waveform.rs`.

## 1. Why the old preview was slow

The first implementation stored one `f32` per asset: a flat 2048-bucket
absolute-peak array. Every frame, for every visible waveform, it walked the
visible pixels, converted each pixel to a peak-bucket range, summed the buckets
in that range, and emitted one `line_segment` per step. Three things were wrong:

- The work was proportional to the clip width and the number of peak buckets per
  pixel on every frame, so it grew as you zoomed out.
- It emitted thousands of individual shapes, which is expensive for `egui`'s
  tessellator.
- It stored only `abs(sample)`, so the draw was a symmetric envelope. At high
  zoom it could not show whether the signal was above or below zero, and it
  could never show individual sample points.

## 2. Data model

`notwig_ui::AudioWaveform` owns the display data for one asset:

- `Arc<AudioAssetBuffer>` for the decoded audio. The arranger and audio editor
  share the same buffer as the engine, so waveform data does not duplicate PCM.
- A signed min/max pyramid: levels at bucket sizes 64, 128, 256, ... up to one
  bucket for the whole asset. Each bucket stores the minimum and maximum
  downmixed sample in that range.

Display samples are the per-frame mean of the asset channels. Stereo material
therefore appears as a single waveform in the clip, which is what the editors
draw today.

`WaveformCache` is a `BTreeMap<AssetId, AudioWaveform>`, stored by `Workspace`
as an `Arc`. `RuntimeView` clones the `Arc`, not the map or the audio, so the
per-frame runtime projection stays cheap regardless of project size.

The pyramid is rebuilt when an asset is loaded, imported, relinked, or
resampled. It is not persisted yet; the bundle still reserves `peaks/` for a
future versioned cache.

## 3. Level selection

`AudioWaveform::level_for(samples_per_pixel)` picks the largest pyramid bucket
that is no larger than the number of source samples covered by one screen pixel.
Each pixel then reads at most two buckets, so the per-frame cost is bounded by
the number of visible pixels, not by the zoom level.

When fewer than 64 samples map to a pixel, the renderer derives each pixel
column's signed extents from the decoded samples instead of the baseline bucket.
The work is bounded by the visible span (at most 64 samples per pixel column),
and the envelope gains detail continuously as the user zooms in.

## 4. Two render modes

Both modes draw a black signed area with a black line:

- **Area envelope** (farther zoom). For each pixel column, read the signed
  `min` and `max` from the selected pyramid level, or from the decoded samples
  below the baseline bucket, and fill the area between them. This is a single
  triangle mesh per waveform, not one shape per column, and it preserves
  asymmetry around zero.
- **Sample graph** (eight samples per pixel or fewer, capped at 8,192 visible
  points). Walk the actual sample points across the visible range, fill the area
  between the graph and the zero axis, and draw a one-pixel line. Segments that
  cross zero are split at the crossing so the fill is two triangles rather than
  a self-intersecting polygon. This is the Bitwig-style close-up detail where
  the individual waveform cycles and sample values are visible.

Both modes clip against the painter's visible rectangle before building any
geometry. A long clip that extends far off-screen therefore contributes only the
pixels that are actually visible; it never builds geometry for the whole clip.
An over-trimmed event still draws only the samples that exist; it never stretches
the waveform.

## 5. Zoom

`Viewport::MAX_PPB` is `1_000_000` pixels per beat. At 120 BPM and 48 kHz one
beat is 24,000 samples, so the maximum zoom is about 41 pixels per sample. That
is enough to reach sample-level detail at normal tempos while keeping
`TimeMus`'s microbeat coordinate space meaningful. The audio editor and arranger
share the same zoom and navigation gestures.

## 6. Invariants and tests

- Waveform data is read-only view data. It never enters `ProjectDoc`.
- Source ranges and sample indices are clamped to the asset buffer.
- Min/max values stay signed; do not collapse them to absolute peaks.
- Drawing cost scales with visible pixels, not with clip length or zoom.
- `crates/notwig-ui/tests/waveform.rs` covers downmixing, signed min/max, and
  pyramid level selection. `crates/notwig-ui/tests/timeline.rs` covers the
  visible source-window mapping.
