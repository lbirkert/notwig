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
- A cached peak hierarchy over power-of-four windows: levels at block sizes 4,
  16, 64, ... up to one block for the whole asset. Each block stores the most
  negative and most positive downmixed sample in that range. A level is built by
  combining four children from the previous level.

Display samples are the per-frame mean of the asset channels. Stereo material
therefore appears as a single waveform in the clip, which is what the editors
draw today.

`WaveformCache` is a `BTreeMap<AssetId, AudioWaveform>`, stored by `Workspace`
as an `Arc`. `RuntimeView` clones the `Arc`, not the map or the audio, so the
per-frame runtime projection stays cheap regardless of project size.

The hierarchy is rebuilt when an asset is loaded, imported, relinked, or
resampled. It is not persisted yet; the bundle still reserves `peaks/` for a
future versioned cache.

## 3. Range peak queries

Every screen span is converted to a source sample span. `AudioWaveform`'s range
peak query then answers the exact min/max of that span without scanning it:

1. Pick the largest aligned 4^n block that fits in the remaining span and
   starts on a block boundary.
2. Combine that block's cached peak, advance past it, and repeat.
3. When no block fits, read decoded samples for the remaining few frames.

At most a few blocks per hierarchy level are consumed, so the query is bounded
by `O(log4 N)` regardless of how many samples the span covers. The block
hierarchy is therefore both the cache and the acceleration structure.

## 4. Render modes

`waveform_mode(samples_per_pixel)` chooses the renderer. At 4096 or fewer samples
per pixel it uses the close sample graph; above that it uses the far peak lines.
Both call the same range-peak query for each screen span.

- **Sample graph** (4096 or fewer samples per pixel). Query one span per pixel
  column, use the signed peak with the larger magnitude from the returned
  min/max, and fill the area between the graph and the zero axis. Segments that
  cross zero are split at the crossing so the fill is two triangles rather than
  a self-intersecting polygon. Below one sample per pixel the graph reads
  decoded samples directly, which is the Bitwig-style close-up detail where
  individual waveform cycles are visible. The graph emits only a fully opaque
  black signed area as a single triangle mesh; it does not stroke a line over
  the fill.
- **Far peak lines** (more than 4096 samples per pixel). Query roughly four
  spans per pixel, read each span's positive and negative peaks, and draw a
  one-pixel-wide vertical line from the zero axis up to the positive peak and
  another from the zero axis down to the negative peak. Nothing is filled
  between the lines, so the far view shows peak strokes instead of a solid band.

Both renderers draw a one-pixel zero line across the visible waveform rectangle
after the waveform geometry, so the baseline stays visible in quiet sections
and in the gaps between far peak lines.

Both renderers clip against the painter's visible rectangle before building any
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
- Cached peaks stay signed; do not collapse them to absolute peaks.
- Drawing cost scales with visible pixels and `O(log4 N)` range queries, not
  with clip length.
- Range queries must return the exact min/max of the requested span. The
  hierarchy uses aligned 4^n blocks and fills the remaining edges with smaller
  blocks or decoded samples.
- Far rendering draws one-pixel vertical lines from zero to the positive and
  negative peaks and no filled area. Close rendering fills the area and adds no
  stroke. Both draw a one-pixel zero line across the visible rectangle.
- Both renderers use the shared opaque black waveform color; the curve fill uses
  the shared `widgets::area` mesh so overlapping segments never accumulate extra
  opacity.
- `crates/notwig-ui/tests/waveform.rs` covers downmixing, power-of-four level
  selection, exact range queries against brute force, area-only close rendering,
  zero-to-peak far lines, and the always-visible zero line.
  `crates/notwig-ui/tests/timeline.rs` covers the visible source-window mapping
  and minimum clip geometry at low zoom.
