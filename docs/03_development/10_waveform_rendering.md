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
- A cached window-peak pyramid for the sample graph: levels at bucket sizes 8,
  16, 32, ... up to the sample-graph threshold. Each bucket stores the signed
  downmixed sample with the largest absolute value in that range. Windows
  combine by keeping the child with the larger magnitude, so the cached peak is
  exact.

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

`AudioWaveform::level_for(samples_per_pixel)` picks the largest min/max pyramid
bucket that is no larger than the number of source samples covered by one screen
pixel. Each pixel then reads at most two buckets, so the per-frame cost is
bounded by the number of visible pixels, not by the zoom level.

`AudioWaveform::window_peak_level_for(samples_per_pixel)` does the same for the
cached peak levels used by the sample graph. The renderer reads one cached peak
per visible bucket instead of scanning the decoded samples for every column.
Below the smallest cached bucket (8 samples) the graph still reads decoded
samples directly, which is cheap because only a few samples map to a pixel.

The sample graph engages at a wider zoom than the pyramid's baseline bucket.
Once one pixel covers more than the sample-graph threshold (256 samples), the
renderer switches to the min/max envelope.

## 4. Two render modes

Both modes emit a fully opaque black signed area as a single triangle mesh. The
close mode draws only the area; it does not stroke a line over the fill.

- **Area envelope** (more than 256 samples per pixel). For each pixel column,
  read the signed `min` and `max` from the selected pyramid level and fill the
  area between them. This preserves asymmetry around zero.
- **Sample graph** (256 or fewer samples per pixel). Walk the cached window-peak
  buckets across the visible range and fill the area between the graph and the
  zero axis. Segments that cross zero are split at the crossing so the fill is
  two triangles rather than a self-intersecting polygon. Using a cached peak
  level keeps each graph point stable when the zoom changes and avoids scanning
  up to hundreds of samples per pixel. This is the Bitwig-style close-up detail
  where individual waveform cycles are visible; at the closest zoom the graph
  reads decoded samples directly.

`waveform_mode(samples_per_pixel)` makes the choice: it returns the sample graph
when one pixel covers at most 256 samples and the area envelope otherwise. The
sample-graph point count is bounded by the visible pixel width, not by the source
sample count.

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
- The sample graph reads cached window-peak levels instead of scanning decoded
  samples per column; the envelope reads the min/max pyramid. Below the smallest
  peak bucket the graph reads decoded samples directly.
- The filled area uses the shared opaque black waveform color; close rendering
  does not add a stroke over the fill. The curve fill uses the shared
  `widgets::area` mesh so overlapping segments never accumulate extra opacity.
- `crates/notwig-ui/tests/waveform.rs` covers downmixing, signed min/max, pyramid
  and window-peak level selection, render-mode thresholds, area-only close
  rendering, and the adaptive sample-graph geometry bound.
  `crates/notwig-ui/tests/timeline.rs` covers the visible source-window mapping
  and minimum clip geometry at low zoom.
