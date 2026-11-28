# Plugin Format Parity Scan

`crates/notwig-plugins/tests/plugin_format_parity.rs` is a manual macOS harness
for installed plugins. It finds plugins that exist in more than one hosted
format and checks that the formats behave like the same plugin: same editor
size and content, same parameter response, and the same rendered audio.

Run it through `scripts/test-plugin-parity.sh`. The script enables the formats
available on the machine, sets the report location, and starts the custom test
harness. `cargo test --workspace` does not run it; the harness skips itself
unless `NOTWIG_PLUGIN_PARITY=1` is set.

Related: [`Testing`](05_testing.md), [`Plugin Hosting`](16_plugin_hosting.md),
and [`Crate Reference`](07_crate_reference.md).

## 1. Scan and grouping

The harness calls `PluginHost::scan(&[])`, which walks the platform's normal
plugin locations. Audio Unit and LV2 use their registries; CLAP, VST3, and VST2
use their standard filesystem paths. Descriptors are normalized by display name
and vendor (case-insensitive, punctuation and whitespace removed).

VST3 scanning often reports an empty vendor. If exactly one non-empty vendor
exists for a name, descriptors with the missing vendor merge into that group;
otherwise the empty-vendor descriptors remain a separate group. `--formats`
restricts the scan, and `--filter` matches a substring of name or vendor.

Only groups with at least two installed formats are tested. Within a group the
harness keeps one descriptor per format, preferring the first path/UID after a
stable sort. Instruments are skipped unless `--include-instruments` is passed; even then
they must expose stereo audio input/output to enter the audio comparison. A
loaded format must report at least two audio inputs and two outputs; otherwise
it is skipped with a warning rather than failing the other formats.

## 2. Editor comparison

For each format that advertises an editor, the harness opens a
`notwig_plugins::NativePluginWindow` (which delegates to
`notwig_rack_editor::EditorWindow`). It pumps the UI for `--gui-settle-ms`,
records the logical content size, and captures the content view as PNG through
`NSBitmapImageRep`.

The first captured format is the reference. For every other format the harness
requires:

- equal logical editor size,
- equal captured pixel dimensions,
- mean per-channel difference no larger than `--gui-mean-tolerance`,
- fraction of pixels above `--gui-pixel-threshold` no larger than
  `--gui-max-fraction`.

Captures are intentionally content-view-only so host title bars do not count.
Some GPU- or layer-backed editors may not render into the offscreen bitmap; the
harness reports capture failure rather than pretending the images matched.
`--no-gui` disables this phase.

## 3. Parameter randomization

Each format's `describe_parameters()` output is mapped by normalized visible
parameter name. Names inside parentheses are ignored, so `Gain (dB)` and `Gain`
match. Only names visible in every format are randomized.

For each round the harness derives one seed from `--seed`, the group key, and
the round number, then generates the same normalized `[0, 1)` value per shared
name. Each format converts that normalized value through its own `ParamSpec`
(`value_from_norm`), so native-unit formats and normalized VST3/VST2 adapters
receive the value their wrapper expects. Parameters that exist in only some
formats are reported as warnings; the intersection is still compared.

Every round calls `prepare` again before setting parameters so each format
starts from the same reset processing state.

## 4. Audio comparison

The harness generates a deterministic stereo signal (two sine tones plus seeded
noise) and renders `--seconds` worth of audio through each format with
`--sample-rate`/`--block-size` (default 48 kHz, 512 frames). Each format's
reported latency is read after preparation, and outputs are compared after
dropping each side's reported latency samples. The first format is the
reference.

A comparison passes when the maximum absolute difference and the RMS difference
are both within `--audio-tolerance` and the reported latencies match. Passing
`--ignore-latency` tolerates latency differences but still aligns by the
reported values. This matters because CLAP latency is not wired through
`notwig-rack` yet and reports zero; a latent plugin will therefore fail a CLAP
versus VST3/AU/LV2 comparison unless latency differences are ignored.

## 5. Report and artifacts

The JSON report is written atomically after every group, so a later plugin
crash does not discard earlier results. Each group records:

- descriptor metadata and capabilities per format,
- parameter count and reported latency,
- per-round audio RMS and all comparison metrics,
- editor sizes, capture dimensions, and GUI comparison metrics,
- warnings, errors, and overall `pass`/`fail`/`skip` status.

By default, screenshots and WAVs are saved only for failed groups, under
`--artifacts/<group>/`. Passing groups keep metrics in JSON only. Use
`--keep-all-artifacts` to retain everything. The script defaults the report to
`target/plugin-parity/run-<timestamp>/report.json`; override it with
`NOTWIG_PLUGIN_PARITY_DIR`, `NOTWIG_PLUGIN_PARITY_REPORT`, or the harness
`--report`/`--artifacts` flags.

The process exits non-zero when any group fails or when no group matches. A
skip is not a failure: it means fewer than two usable formats or editors
remained after filtering.

## 6. Current limits

- The harness runs all plugins in one process. A hard plugin crash ends the
  run; the JSON report up to the previous group remains.
- Editor capture uses the view bitmap path, not a screen-recording API, so
  plugins that draw exclusively through GPU layers may produce blank captures.
- Grouping is heuristic. Plugins whose display names or vendors differ between
  formats will not be compared automatically; use `--filter` to inspect and
  extend the grouping rule if that becomes common.
- Parameter matching is name-based. Formats that expose completely different
  parameter names still render audio with default state for the unmatched
  controls and report the mismatch as a warning.
