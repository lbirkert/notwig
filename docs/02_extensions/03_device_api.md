# Device API

An extension device implements `DeviceExtension` and registers itself from the
macro entrypoint:

```rust
use notwig_ext_api::{DeviceExtension, Extension, notwig_extension};

#[notwig_extension]
pub fn extension(extension: &mut Extension) {
    extension.register_device::<MyDevice>();
}
```

The full trait is:

```rust
pub trait DeviceExtension: Send + Sized + 'static {
    fn new() -> Self;
    fn spec() -> notwig_dsp::DeviceSpec;
    fn draw(&mut self, ui: &mut notwig_ext_api::UiContext<'_>);
    fn prepare(&mut self, _sample_rate: u32, _channels: u16) {}
    fn set_param(&mut self, _param_id: notwig_core::ParamId, _value: f64) -> bool {
        false
    }
    fn process(&mut self, samples: &mut [f32]);
    fn latency(&self) -> u32 {
        0
    }
}
```

## Lifecycle

1. `spec()` is called once when the host registers the device type.
2. `new()` creates an instance when the device is inserted or the engine is
   rebuilt.
3. `prepare(sample_rate, channels)` is called before the instance processes
   audio, and again when the engine format changes.
4. `set_param(id, value)` is called for host parameter changes.
5. `process(samples)` runs in the audio callback.
6. `draw(ui)` runs once per UI frame while the panel is visible.
7. `latency()` is queried by the engine for delay compensation.

The host keeps one `UiMemory` per instance. Device state that is not a parameter
belongs in the device struct and is rebuilt if the engine rebuilds instances.

## Device spec

`DeviceSpec` describes the device to the host and the device browser:

```rust
fn spec() -> DeviceSpec {
    DeviceSpec {
        type_id: TYPE_ID.into(),
        version: SchemaVersion::V0,
        kind: DeviceKind::AudioFx,
        ports: vec![
            Port {
                id: "in".into(),
                dir: PortDir::In,
                signal: SignalType::Audio(2),
                name: "Input".into(),
                optional: false,
            },
            Port {
                id: "out".into(),
                dir: PortDir::Out,
                signal: SignalType::Audio(2),
                name: "Output".into(),
                optional: false,
            },
        ],
        params: vec![ParamSpec::continuous(
            gain_id(),
            "Gain",
            Unit::Decibels,
            -24.0,
            24.0,
            0.0,
        )],
        latency_samples: 0,
        name: "My Gain".into(),
        vendor: "Example".into(),
        ui: DeviceUiSpec::custom(format!("ext:{TYPE_ID}"), Some(180.0)),
    }
}
```

- `type_id` must match the manifest device type ID.
- `kind` selects the browser category and chain behavior. Common values are
  `AudioFx`, `Instrument`, and `Container`.
- `ports` declare the audio shape. Extension devices currently work with
  interleaved stereo audio.
- `latency_samples` is the static latency claim. Use `latency()` when the value
  changes at runtime.
- `ui` chooses the generic view or a custom extension panel.

## Parameters

Declare parameters with the constructors in `notwig_dsp::ParamSpec`:

```rust
ParamSpec::continuous(gain_id(), "Gain", Unit::Decibels, -24.0, 24.0, 0.0)
ParamSpec::boolean(bypass_id(), "Bypass", false)
ParamSpec::enumeration(mode_id(), "Mode", 4, 0)
```

`Unit` selects display formatting and, for hertz, a logarithmic normalized map.
`ParamSpec::continuous` uses a linear map except for hertz.

Rules:

- Parameter IDs must be stable across versions and sessions.
- The same `ParamId` must be used in `spec`, `set_param`, and `draw`.
- Parameter values are physical values in `set_param`, not normalized values.
- Normalized values are always in `0..=1` at the panel boundary.
- Clamp values in `set_param`; the host validates the action, not the DSP
  behavior.
- The device owns its smoothing. Parameter changes arrive at block boundaries.

Fixed UUID integers are a good choice for IDs because they are stable and need no
randomness at runtime:

```rust
fn gain_id() -> ParamId {
    ParamId::from_uuid(Uuid::from_u128(
        0x6761_696e_0000_0000_0000_0000_0000_0001,
    ))
}
```

## DSP

`process` receives interleaved `f32` samples in extension linear memory:

```rust
fn process(&mut self, samples: &mut [f32]) {
    for sample in samples {
        *sample *= self.gain;
    }
}
```

- Process in place. The buffer is already the correct channel count.
- Do not allocate, lock, block, or perform I/O in `process`.
- Keep all per-instance DSP state in the device struct.
- Do not assume a fixed block size. The host may split large blocks.
- Return promptly. Fuel limits apply to Limited and DrawOnly trust levels.

`prepare` is the place to compute coefficients that depend on sample rate or
channel count:

```rust
fn prepare(&mut self, sample_rate: u32, channels: u16) {
    self.channels = usize::from(channels.max(1));
    self.update_coefficients(f64::from(sample_rate.max(1)));
}
```

## Metering

The engine measures the signal before and after every device and exposes it to
the panel as a `DeviceMeterSnapshot` through `UiContext::device_meter()`. The
four fields are linear peak amplitudes:

- `input_peak_l`, `input_peak_r`
- `output_peak_l`, `output_peak_r`

Use the ratio of output to input to display device gain reduction. The panel is
a separate extension instance from the audio processor, so display state must
come from this snapshot rather than from fields updated in `process`. Values are
zero while playback is stopped.

## Latency

Return nonzero latency only when the device actually delays its output. The
engine uses the reported value for plug-in delay compensation across tracks,
groups, and nested chains.

```rust
fn latency(&self) -> u32 {
    self.delay_samples
}
```

## Host parameter changes

`set_param` receives a physical value. Return `true` only when the ID belongs to
the device:

```rust
fn set_param(&mut self, param_id: ParamId, value: f64) -> bool {
    if param_id != gain_id() {
        return false;
    }
    self.gain_db = value.clamp(-24.0, 24.0);
    self.update_gain();
    true
}
```

The host uses the return value to decide whether the parameter was accepted.
Unrecognized IDs must not change device state.

## Current limits

Extension devices process audio only. Note events, MIDI input, and persistent
device state blobs are not forwarded to extension devices yet. Store user-facing
state as declared parameters so the host can persist and restore it through the
normal parameter path.

Next: [Panel API](04_panel_api.md).
