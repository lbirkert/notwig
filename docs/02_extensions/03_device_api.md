# Device API

An extension device implements `DeviceExtension` and registers itself from the
macro entrypoint:

```rust
use notwig_ext_api::{EP_NOTWIG_DEVICES, DeviceExtension, ExtensionBuilder, notwig_ext};

#[notwig_ext(load)]
pub fn load(extension: &mut ExtensionBuilder) {
    extension.register_extension::<MyDevice>(EP_NOTWIG_DEVICES);
}
```

`MyDevice` must implement `Default` and opt into registration with the derive:

```rust
#[derive(Default, notwig_ext_api::ExtensionCall)]
#[extension_call(DeviceExtension)]
struct MyDevice;
```

Panels use the guest egui API. Implement `draw` with the shared
`notwig-ui` components; see
[extension panels](../03_development/19_extension_system.md).

The full trait is:

```rust
pub trait DeviceExtension: Send + Sized + 'static {
    fn new() -> Self;
    fn spec() -> notwig_core::dsp::DeviceSpec;
    fn draw(
        &mut self,
        ui: &mut notwig_ext_api::egui::Ui,
        meter: notwig_ext_api::DeviceMeterSnapshot,
    );
    fn prepare(&mut self, _sample_rate: u32, _channels: u16) {}
    fn set_param(&mut self, _param_id: notwig_core::ParamId, _value: f64) -> bool {
        false
    }
    fn note_event(&mut self, _event: notwig_ext_api::NoteEvent) {}
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
5. `note_event(event)` is called for each note event before the block is
   processed. Events are ordered by block sample offset.
6. `process(samples)` runs in the audio callback.
7. `draw(ui, meter)` runs once per UI frame while the panel is visible.
   The host composites the returned panel frame.
8. `latency()` is queried by the engine for delay compensation.

The extension runtime keeps one guest egui context per UI instance. Device state
that is not a parameter belongs in the device struct and is rebuilt if the
engine rebuilds instances.

## Runtime and ABI rules

- `process` runs on the audio path. Do not allocate, lock, block, perform I/O,
  or log from it. Collect diagnostics and report them from a later host call or
  through a parameter instead.
- The generated ABI catches unwinding panics: status calls return
  `AbiStatus::Error`, pointer calls return null/empty, and destroy is swallowed.
  A `panic = "abort"` build still aborts, and a WASM trap is not recoverable, so
  validate inputs and avoid `unwrap`/`panic` in callbacks.
- The host may keep one instance for DSP and a separate instance for the panel.
  Calls on one handle are serialized and are not reentrant. Do not call back
  into the same device handle.
- `notwig.host.log` and `notwig.host.notify` are available to WASM entrypoints,
  including the browser host. They decode text lossily, ignore invalid
  pointer/length pairs, and copy at most 8 KiB per call, marking longer messages
  as truncated. Decoding is skipped when no tracing subscriber enables the
  level, but they are still not a substitute for real-time-safe logging.
- The ABI version must match the host exactly. Changing an export signature, a
  wire layout, or this contract requires an ABI bump and a rebuild; there is no
  compatibility negotiation.
- Native wasmtime enforces a 64 MiB memory limit and 20 million fuel per call.
  The browser host relies on the browser sandbox and does not configure an
  explicit memory or fuel cap. Native libraries are trusted and unsandboxed.

The runtime design and version policy are specified in
[`Extension System`](../03_development/19_extension_system.md).

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
        container: None,
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

Declare parameters with the constructors in `notwig_core::dsp::ParamSpec`:

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

`process` receives interleaved `f32` samples in place. The same signature is
used by WASM and native builds:

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
- Return promptly. WASM under the native wasmtime host runs with per-call fuel
  metering and a memory limit; the browser host has neither and native
  libraries run unmonitored.

`prepare` is the place to compute coefficients that depend on sample rate or
channel count:

```rust
fn prepare(&mut self, sample_rate: u32, channels: u16) {
    self.channels = usize::from(channels.max(1));
    self.update_coefficients(f64::from(sample_rate.max(1)));
}
```

## Metering

The engine measures the signal before and after every device and passes a
`DeviceMeterSnapshot` to `draw`. The four fields are linear peak
amplitudes:

- `input_peak_l`, `input_peak_r`
- `output_peak_l`, `output_peak_r`

Use the ratio of output to input to display device gain reduction. The panel is
a separate extension instance from the audio processor, so display state must
come from this snapshot rather than from fields updated in `process`. Values are
zero while playback is stopped. Draw the snapshot with the shared
`notwig_ui::draw_stereo_meter_with_peak` helper.

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

## Note events

Instrument and note-effect devices receive note events through
`note_event`. The event uses `notwig_ext_api::NoteEvent`:

```rust
pub struct NoteEvent {
    pub sample_offset: u32,
    pub kind: NoteKind, // On or Off
    pub pitch: u8,
    pub velocity: f32,
}
```

`sample_offset` is relative to the start of the upcoming block. `note_event`
runs before `process`, so an instrument can queue events and apply them at the
matching frame while synthesizing. The host truncates events if a block contains
more than can fit in the prepared event buffer.

Declare an instrument with `DeviceKind::Instrument` and a `NoteEvent` input port.
Instrument tracks may host at most one instrument.

## Containers

A device with `DeviceKind::Container` gets a host-owned child chain. The host
owns routing, nesting, latency, nested parameters, and persistence; the
extension supplies only the stage, parameters, and panel.

```rust
use notwig_core::dsp::{ContainerRouting, ContainerSpec};

fn spec() -> DeviceSpec {
    DeviceSpec {
        kind: DeviceKind::Container,
        // ...
        container: Some(ContainerSpec {
            routing: ContainerRouting::StageThenChildren,
            wet_param: Some(mix_id()),
        }),
        // ...
    }
}
```

- `StageThenChildren` (default) runs `process`, then the child chain. Use it
  when the chain should process the device output, such as a wet FX chain after
  a reverb.
- `ChildrenThenStage` runs the child chain, then `process`.
- `wet_param = Some(id)` makes the container a wet path. The input is the dry
  signal, the stage and child chain are the wet path, and the host crossfades
  with the parameter's `0..=1` value. The stage's `process` must output the wet
  signal only; do not apply dry/wet inside `process`. The parameter must also be
  declared in `params`.
- When no stage can be created, the child chain still processes and no wet blend
  is applied.

The stock `notwig.chain` device is separate and keeps its own processor. Users
add or remove child devices through the normal device browser and the chain tab
on the container card. Extensions cannot inspect or reorder child devices.

Containers are serial only today. Parallel/split paths and sidechain inputs
need a later routing ABI.

## Current limits

Persistent device state blobs are not forwarded to extension devices yet. Store
user-facing state as declared parameters so the host can persist and restore it
through the normal parameter path.

Next: [Panel API](04_panel_api.md).
