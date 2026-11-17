# Stock Devices (`notwig-dsp`, v0)

All conform to device/parameter/RT contracts.

## 0. No Stock Instrument (v0)
- v0 ships no built-in synth or sampler. Instrument tracks must host a VST3/AU instrument (VST2 fallback). A stock instrument is deferred.

## 1. Tool — `typeId: "notwig.tool"` (AudioFx)
- **Ports:** Audio in/out (mono/stereo capable, matched layout).
- **Params:** `inputGainDb`, `outputGainDb`, `pan`/`balance` (constant-power pan law, -3 dB at center), `stereoWidth`, `polarityL`, `polarityR`, `dryWet`.
- **Behavior:** correct gain-staging/stereo utility, smooth transitions, RT-safe meter taps.
- **State:** parameter values (+ version).

## 2. Equalizer — `typeId: "notwig.eq"` (AudioFx)
- **Ports:** stereo audio in/out baseline.
- **Params:** fixed per-version band set `b1..bN` each `{enabled,type(Bell|LowShelf|HighShelf|HPF|LPF),freqHz,gainDb,q,slope?}` plus output trim as specified.
- **Behavior:** stable parametric EQ; smoothed coefficient updates; combined response consistent with DSP.
- **Analysis:** optional pre/post spectrum data via RT-safe analysis path for UI projection.
- **State:** all band/global params (+ version).

## 3. Chain — `typeId: "notwig.chain"` (Container/AudioFx)
- **Ports:** external audio in/out compatible with internal chain.
- **Params:** `inputGainDb`, `outputGainDb`, `dryWet`.
- **Structure:** owns internal ordered child chain; supports insert/remove/reorder/bypass and nesting.
- **State:** child order, child states/params, container params; reports aggregate latency for PDC.
- **Behavior:** dry/wet aligned with compensated internal latency.

## 4. Common
- Stable `ParamId`s, versioned state/presets.
- Bypass is click-free and latency-compensated: dry passthrough with the device's reported latency retained for PDC.
- Faust-preferred implementations with Rust integration; interoperable with hosted plugins in the same chains.
