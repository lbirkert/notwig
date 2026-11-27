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
- A cached window-peak pyramid for close rendering: levels at bucket sizes 8,
  16, 32, ... up to one bucket for the whole asset. Each bucket stores the
  signed downmixed sample with the largest absolute value in that range. Windows
  combine by keeping the child with the larger magnitude, so the cached peak is
  exact.
- A cached min/max pyramid for far rendering: levels at bucket sizes 64, 128,
  256, ... up to one bucket for the whole asset. Each bucket stores the most
  negative and most positive downmixed sample in that range.

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

`AudioWaveform::window_peak_level_for(samples_per_pixel)` picks the largest
cached peak bucket that is no larger than the number of source samples covered
by one screen pixel. The renderer reads one cached peak per visible bucket
instead of scanning the decoded samples for every column, so the per-frame cost
is bounded by the number of visible pixels, not by the zoom level.

Below the smallest cached bucket (8 samples) the graph reads decoded samples
directly, which is cheap because only a few samples map to a pixel.

`AudioWaveform::far_level_for(samples_per_pixel)` picks the largest min/max
bucket for the far renderer with the same rule. Each pixel column then reads at
most two buckets.

## 4. Render modes

`waveform_mode(samples_per_pixel)` chooses the renderer. At 256 or fewer samples
per pixel it uses the close sample graph; above that it uses the far peak lines.

- **Sample graph** (256 or fewer samples per pixel). Walk the cached window-peak
  buckets across the visible range and fill the area between the graph and the
  zero axis. Segments that cross zero are split at the crossing so the fill is
  two triangles rather than a self-intersecting polygon. Using a cached peak
  level keeps each graph point stable when the zoom changes and avoids scanning
  many samples per pixel. At the closest zoom the graph reads decoded samples
  directly, which is the Bitwig-style close-up detail where individual waveform
  cycles are visible. The graph emits only a fully opaque black signed area as a
  single triangle mesh; it does not stroke a line over the fill.
- **Far peak lines** (more than 256 samples per pixel). For each visible pixel
  column, read the positive and negative peaks from the min/max pyramid. Draw a
  one-pixel-wide vertical line from the zero axis up to the positive peak and
  another from the zero axis down to the negative peak. Nothing is filled between
  the lines, so the far view shows per-column peak strokes instead of a solid
  band.

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
- Drawing cost scales with visible pixels, not with clip length or zoom.
- The sample graph reads cached window-peak levels instead of scanning decoded
  samples per column. Below the smallest peak bucket the graph reads decoded
  samples directly.
- Far rendering draws one-pixel vertical lines from zero to the positive and
  negative peaks and no filled area. Close rendering fills the area and adds no
  stroke.
- Both renderers use the shared opaque black waveform color; the curve fill uses
  the shared `widgets::area` mesh so overlapping segments never accumulate extra
  opacity.
- `crates/notwig-ui/tests/waveform.rs` covers downmixing, window-peak and far
  peak level selection, area-only close rendering, and zero-to-peak far lines.
  `crates/notwig-ui/tests/timeline.rs` covers the visible source-window mapping
  and minimum clip geometry at low zoom.
