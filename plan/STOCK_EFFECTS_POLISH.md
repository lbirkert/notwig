# Stock effects polish and new extension devices

Status: implementation plan. Scope is `notwig-ext-devices` and the shared
extension panel/dynamics helpers.

## Goals

- Gate and Limiter histories at 60 Hz with more retained points and a clearer
  gain-reduction curve.
- New devices: Compressor, Clipper, Delay, Chorus, Transient Control.
- Reverb: more diffusion plus Room/Hall/Plate type selection.
- Keep every panel inside the fixed device card and every DSP path real-time
  safe (no allocation in `process`).
- Keep type ids and parameter ids stable; add new ids only.

## Shared work

### History

- `HISTORY_LEN` 50 -> 128.
- Push samples at 60 Hz, repaint panels every 16 ms.
- Draw the gain curve with a glow plus a sharper line. Limiter shows the gain
  curve in addition to its follow-gain threshold.
- Keep the existing dB range (-60..0) and history graph placement.

### Panel labels

- Fix `ui::knob_label` clipping so labels are visible below knob rects.
- Reuse the shared label helper in Reverb and the new devices.

### DSP helpers

Add a small `util` module:

- dB/gain conversion and denormal flushing;
- one-pole smoother/filter;
- stereo delay line with fractional reads;
- soft-clip curves and a DC blocker.

No new dependencies.

## Devices

| Device | Type id | Kind | Core parameters |
| --- | --- | --- | --- |
| Compressor | `notwig.ext.devices.compressor` | AudioFx | Threshold, Ratio, Attack, Release, Knee, Makeup, Mix |
| Clipper | `notwig.ext.devices.clipper` | AudioFx | Drive, Knee/softness, Tilt, Output, Mix |
| Delay | `notwig.ext.devices.delay` | AudioFx | Time L/R, Feedback, Crossfeed, Damping, Ducking, Width, Mix |
| Chorus | `notwig.ext.devices.chorus` | AudioFx | Mode, Rate, Depth, Feedback, Delay, Width, Mix |
| Transient | `notwig.ext.devices.transient` | AudioFx | Attack Gain/Time, Sustain Gain/Time, Mode, Mix |
| Reverb | `notwig.ext.devices.reverb` | Container | Type, Size, Damping, Pre-delay, Width, Mix |

### Compressor

Stereo-linked peak detector, soft knee, attack/release-smoothed gain reduction,
makeup gain, dry/wet. Reuse the history graph and meter style from Limiter/Gate.
Panel: graph, meter, two compact knob rows.

### Clipper

Drive into a soft clip, knee blends hard/soft, tilt balances low/high bands, DC
blocker, output gain, dry/wet. Panel includes a transfer-curve preview.

### Delay

Two fractional delay lines with feedback, crossfeed, damping, dry-input ducking,
stereo width, and mix. Free time only; tempo sync is out of scope until the ABI
exposes transport. Panel: stereo meter plus two knob rows.

### Chorus

Modulated stereo delay with sine LFOs, width/phase spread, feedback, and mix.
Mode switches between chorus and flanger ranges. Panel: graph/LFO indicator plus
compact knobs.

### Transient Control

Fast and slow stereo-linked envelope followers detect transients. Attack gain
and time shape the fast component; sustain gain and time shape the body. Mode
switches peak/RMS detection. Panel: attack/sustain sections, mode dropdown, mix.

### Reverb

- Add a fixed pre-diffusion allpass network and extra post-allpasses; increase
  allpass feedback for denser tails.
- Add `Type` (Room, Hall, Plate). Types change comb/allpass effective lengths,
  feedback, damping, and diffusion without reallocating in `set_param`.
- Panel: Type dropdown, compact graph/meter, and five knobs.

## Order

1. Shared history, label, and util changes.
2. Compressor and Clipper.
3. Delay and Chorus.
4. Transient Control.
5. Reverb type/diffusion upgrade.
6. Manifest/docs/readme and ABI host test.
7. Full verification.

## Verification

- Device tests: spec, parameter rejection/clamping, panel frame validation, and
  one or more DSP behavior assertions per device.
- Reverb tests: existing tail/predelay plus type switching and diffusion.
- `crates/notwig-ext/notwig-ext/tests/limiter_extension.rs`: updated device count
  and spec assertions.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, WASM release build + bundle, `mdbook build`.
