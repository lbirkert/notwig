# Extension devices for `notwig-ext-devices`

Status: planning note. The crate currently ships Limiter, Gate, Reverb, Synth,
and Gain. Reverb is a container with a host-owned wet FX chain; see
[`plan/EXTENSION_CHAINS.md`](EXTENSION_CHAINS.md). This page lists the devices
worth adding next, what the current extension ABI can and cannot express, and a
sensible implementation order.

Related:

- [`plan/EXTENSIONS.md`](EXTENSIONS.md) — extension system plan and ABI history.
- [`plan/EXTENSION_CHAINS.md`](EXTENSION_CHAINS.md) — container/chain support.
- [`plan/MOVING_DEVICES.md`](MOVING_DEVICES.md) — whether core stock devices
  should move here.
- [`docs/02_extensions/03_device_api.md`](../docs/02_extensions/03_device_api.md)
  — author-facing device contract.

## What the ABI allows today

Everything below must be expressible with the current `DeviceExtension` contract:

- `spec()` declares stable parameters; `set_param` receives physical values.
- `new`/`prepare` allocate; `process` transforms one interleaved stereo block;
  `note_event` receives note on/off for instruments.
- `draw` renders a guest egui panel from parameters plus a
  `DeviceMeterSnapshot` (four linear peak values).

Known hard limits, all of which constrain the candidate list:

- **No persistent state blobs.** User-visible state must be parameters. No
  presets, no sample/wavetable data, no convolution impulse responses.
- **No project or package assets.** Manifest includes exist, but no guest API
  reads them.
- **Panel and processor are different instances.** Only the meter snapshot
  crosses from engine to panel; there is no arbitrary processor-to-panel data
  channel. A scope/spectrum cannot display data computed in `process` today.
- **Stereo interleaved only.** No sidechain input, no multi-port routing, no
  mono or surround processing. `ports` are metadata for the browser.
- **No transport/tempo in the public API.** The wire carries
  `sample_position`/`playing` and `prepare` carries `max_block`, but the generated
  wrappers discard them. Tempo-synced delays and LFOs must wait.
- **No note output.** A device receives notes but cannot emit them, so Note FX,
  arpeggiators, and sequencers are not possible.
- **Block-boundary parameters.** No per-sample ramps; devices own smoothing.
- **WASM budget.** Native uses 64 MiB and 20M fuel per call; heavy FFT or
  high-ratio oversampling per block can exceed the budget. Allocate in
  `prepare`, keep per-block work bounded.

## Ready without host changes

These fit the current ABI. Effort is relative to the existing Limiter/Gate code.

| Device | Type id | Why | Effort |
| --- | --- | --- | --- |
| Compressor | `notwig.ext.devices.compressor` | Natural sibling of Limiter/Gate; reuses the dynamics graph. | S |
| Saturation / Distortion | `notwig.ext.devices.saturator` | Small DSP, strong sound design value, parameter-only. | S |
| State-variable Filter | `notwig.ext.devices.filter` | Cutoff/resonance/type, simple and robust. | S |
| Chorus / Phaser | `notwig.ext.devices.chorus` | Modulated delay, no assets, no tempo sync needed. | M |
| Stereo Delay | `notwig.ext.devices.delay` | Fixed-time delay with feedback and wet/dry. Add tempo sync later. | M |
| Algorithmic Reverb | `notwig.ext.devices.reverb` | Implemented. Freeverb-style with a host-owned wet FX chain. | Done |
| Stereo Imager | `notwig.ext.devices.imager` | Mid/side width, mono-below frequency, balance. | S |
| Drum Synth | `notwig.ext.devices.drums` | Note-event instrument with a small voice set. | M |
| Synth v2 | `notwig.ext.devices.synth` | Extend the existing synth in place. | M |
| Test Generator | `notwig.ext.devices.testgen` | Sine/noise/sweep for debugging and calibration. | S |

### Compressor

Type id `notwig.ext.devices.compressor`, `DeviceKind::AudioFx`.

- Parameters: threshold (-60..0 dB), ratio (1:1..20:1), attack (0.1..100 ms),
  release (10..1000 ms), knee (0..24 dB), makeup (-24..24 dB), mix (0..1),
  optional sidechain-filter placeholder disabled for now.
- Stereo-linked peak or RMS detector; soft knee; smoothed gain reduction;
  makeup gain; dry/wet mix.
- Panel: reuse `dynamics.rs` (same history graph as Limiter/Gate), show gain
  reduction, threshold line, ratio readout, and a stereo output meter.
- Report static latency 0. Keep the detector and gain smoothing allocation-free
  in `process`.
- Tests: spec, parameter rejection/clamping, below-threshold transparency,
  ratio accuracy on a known step, attack/release timing, frame generation.

### Saturation / Distortion

Type id `notwig.ext.devices.saturator`, `DeviceKind::AudioFx`.

- Parameters: drive (0..36 dB), character/type (soft clip, tanh, fold, bit
  crush), tone (tilt filter), output (-24..24 dB), mix.
- DC-block after asymmetric curves; optional 2x oversampling with a short
  polyphase or biquad-based anti-image filter. Bound oversampling so the WASM
  budget holds.
- Panel: transfer-curve preview driven by parameters (draw the curve from the
  selected character and drive; no audio data needed), plus input/output meter.
- Tests: bypass at zero drive/mix, bounded output, curve continuity, no NaNs,
  frame generation.

### State-variable Filter

Type id `notwig.ext.devices.filter`, `DeviceKind::AudioFx`.

- Parameters: type (LP/HP/BP/notch), cutoff (20 Hz..20 kHz, log), resonance
  (0..1), slope (12/24 dB), drive, mix.
- Topology-preserving state-variable filter or cascaded biquads. Recompute
  coefficients on parameter change; smooth cutoff/resonance.
- Panel: small magnitude-response sketch computed from the same coefficients,
  meter, and the shared knobs.
- Tests: coefficient stability at the extremes, unity passband, attenuation
  slope, parameter changes without discontinuities.

### Chorus / Phaser

Type id `notwig.ext.devices.chorus`, `DeviceKind::AudioFx`.

- Parameters: mode (chorus/flanger/phaser), rate (0.05..8 Hz), depth, feedback,
  delay (chorus/flanger), spread, mix.
- Modulated delay lines or all-pass chain, LFO per channel with phase offset.
  Allocate the delay buffer in `prepare`; interpolate read positions.
- Panel: two LFO phase indicators and a stereo meter. No transport needed; free
  running LFOs only until tempo sync exists.
- Tests: bounded feedback, no clicks on rate change, stereo decorrelation,
  bypass at zero depth.

### Stereo Delay

Type id `notwig.ext.devices.delay`, `DeviceKind::AudioFx`.

- Parameters: left/right time (1..2000 ms), feedback, cross-feed, damping,
  input width, mix.
- Two delay lines allocated in `prepare` from the maximum delay; fractional
  read with linear interpolation; damping low-pass in the feedback path; clamp
  feedback below unity.
- Panel: two time readouts, feedback/mix knobs, stereo meter.
- Tempo-synced notes are a follow-up once the ABI exposes tempo/transport.
- Tests: exact delay length at a known sample rate, feedback decay, no runaway,
  prepare at different sample rates.

### Algorithmic Reverb

Implemented: type id `notwig.ext.devices.reverb`, `DeviceKind::Container`.

- Parameters: size, damping, pre-delay (0..200 ms), width, mix.
- Freeverb-style 8-comb/4-allpass network per channel with a stereo predelay.
  Buffers allocate in `prepare`; the comb feedback path flushes denormals.
- `ContainerRouting::StageThenChildren` with `Mix` as the wet parameter. The
  host owns the child chain, so nested devices process the reverb wet signal
  before the dry/wet blend. `process` outputs wet only.
- Panel: decay-curve graph, stereo output meter, and shared knobs.
- Tests: container spec, pre-delay, finite decaying tail, reset, and parameter
  validation.

### Stereo Imager

Type id `notwig.ext.devices.imager`, `DeviceKind::AudioFx`.

- Parameters: width (0..2), balance, mono-below (20..500 Hz), output.
- Mid/side encode, optional low-frequency mono fold, decode. Zero latency.
- Panel: simple mid/side vector scope drawn from meter values, width/balance
  knobs.
- Tests: width 1 is identity, width 0 is mono, low-cut behavior.

### Drum Synth

Type id `notwig.ext.devices.drums`, `DeviceKind::Instrument`.

- A fixed small kit: kick, snare, closed/open hat, clap, tom. Map pitches from
  one octave (e.g. C1..B1) to voices; ignore pitches outside the map.
- Parameters: per-voice level/tune/decay plus global volume, and a shared
  accent amount.
- Voices are simple sine/noise envelopes with a short pitch drop for kick/toms.
  Allocate per-voice state in `new`/`prepare`; keep a fixed voice count.
- Panel: pad row with note names, level/tune/decay controls for the selected
  voice, stereo meter.
- Tests: note map, voice stealing/retrigger, decay length, velocity scaling,
  all-notes-off behavior on stop.

### Synth v2

Extend `notwig.ext.devices.synth` rather than adding a second synth type id.

- Add a resonant low-pass filter with its own ADSR, oscillator unison
  (2..8 voices, detune, stereo spread), pulse width for square, and glide.
- Keep the existing parameter IDs stable; add new IDs. Old projects must keep
  loading with defaults for the new parameters.
- Panel: oscillator/filter/envelope sections with the shared dropdown and knobs.
- Tests: existing synth tests plus filter response, unison gain normalization,
  and glide timing.

### Test Generator

Type id `notwig.ext.devices.testgen`, `DeviceKind::AudioFx`.

- Parameters: signal (sine, white noise, pink noise, sweep), frequency/period,
  level, channel mode.
- Useful for calibrating levels and testing the chain. Keep it simple and
  bounded; no transport dependence in v1.
- Tests: frequency accuracy, level accuracy, deterministic noise with a seed
  parameter.

## Blocked on host features

Do not start these until the prerequisite lands; otherwise they become
hard-coded workarounds.

| Device | Prerequisite |
| --- | --- |
| Oscilloscope | An engine-to-panel analysis snapshot (waveform ring) in `DeviceDrawContext`. Meter peaks are not enough, and the panel is a separate instance. |
| Spectrum analyzer | Same analysis channel, plus a guest FFT (add a small dependency or a radix-2 implementation) and a bounded per-frame history. |
| Tempo-synced delay/LFO | Tempo and transport state in the public device API and ABI. |
| Convolution reverb | Extension asset access or a state-blob/asset ABI, plus IR loading off the audio thread. |
| Wavetable/sample instrument | Asset access and state persistence. |
| Arpeggiator / Note FX | A note-output path in the ABI (currently notes only go in). |
| Sidechain compressor | Multi-port routing and a sidechain input; `ports` are metadata today. |
| Parallel/split container | Serial containers are implemented; parallel paths and sidechains need a multi-port ABI. |

## Packaging and quality bar

Every added device should include:

- a stable type id under `notwig.ext.devices.*` and fixed parameter UUIDs;
- an entry in `extension.json` and a section in the crate `README.md`;
- `DeviceSpec` with `DeviceUiSpec::custom("ext:<type_id>", width)` and a panel
  drawn with `notwig_ui` components;
- static `latency_samples` where applicable and a runtime `latency()` when it
  changes;
- no allocation, locking, logging, or I/O in `process`;
- parameter clamping in `set_param`, smoothing in the device, and denormal-safe
  state where feedback is involved;
- unit and integration tests in `crates/notwig-ext/notwig-ext-devices/tests/`
  covering spec, parameter rejection, DSP behavior, and panel frame generation;
- a pass through `crates/notwig-ext/notwig-ext/tests` so the device is loaded
  through the real WASM and native ABIs.

## Suggested order

1. Compressor — highest value, lowest risk, reuses existing dynamics code.
2. Saturation and Filter — small, broadly useful, exercise different DSP shapes.
3. Chorus and Stereo Delay — modulation and memory management.
4. Drum Synth and Synth v2 — instrument coverage.
5. Stereo Imager and Test Generator — utility polish.
6. Analysis devices only after the engine-to-panel analysis channel exists.

Reverb and serial container chains are done. An `FX Chain`/`Rack` example could
now be built with a pass-through stage and no wet parameter.
