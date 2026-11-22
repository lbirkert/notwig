use crate::clock::Instant;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::{DeviceId, RtEvent, TrackId};
use notwig_engine::{ControlHandle, Engine, EngineConfig};
use notwig_ext_api::DeviceMeterSnapshot;
use notwig_ui::MeterValues;

use crate::audio_input::{InputHub, InputRequest, available_input_options};
use crate::recording::RecordingSession;

/// Synthetic device id that always resolves to the host's default output
/// device. It is selectable in Settings instead of relying on an empty device
/// string to imply the system default.
pub const SYSTEM_OUT_DEVICE_ID: &str = "system-out";

const SYSTEM_OUT_LABEL: &str = "System Out";
const SYSTEM_OUT_WARNING: &str = "System Out follows the operating system's default output device.";
const COMMON_SAMPLE_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/// Errors that mean the current output stream cannot continue and the app must
/// reopen the configured device. `DeviceChanged` is handled separately because
/// the host may keep the rerouted stream alive.
#[doc(hidden)]
#[must_use]
pub fn is_device_loss_error(kind: cpal::ErrorKind) -> bool {
    matches!(
        kind,
        cpal::ErrorKind::DeviceNotAvailable
            | cpal::ErrorKind::StreamInvalidated
            | cpal::ErrorKind::HostUnavailable
    )
}

#[doc(hidden)]
#[must_use]
pub fn is_device_change_error(kind: cpal::ErrorKind) -> bool {
    kind == cpal::ErrorKind::DeviceChanged
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioSettings {
    pub driver: String,
    pub device: String,
    pub sample_rate: Option<u32>,
    pub block_size: u32,
}

const DSP_HISTORY: usize = 200;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioSnapshot {
    pub running: bool,
    pub driver: String,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub block_size: u32,
    pub playhead_samples: u64,
    #[serde(skip)]
    pub playhead_updated: Option<Instant>,
    /// Age of `playhead_samples` at the time the engine process serialized the
    /// snapshot. Used by the UI process to preserve the audio callback's
    /// timestamp across IPC instead of re-timestamping a delayed snapshot.
    #[serde(default)]
    pub playhead_age_nanos: u64,
    pub playing: bool,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub device_meters: BTreeMap<DeviceId, DeviceMeterSnapshot>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub error: Option<String>,
    pub crashed: bool,
    /// The output stream was invalidated because its device disappeared or the
    /// host became unavailable. This is recoverable: the app pauses transport,
    /// rescans devices, and retries opening the configured output.
    #[serde(default)]
    pub device_lost: bool,
    #[serde(default)]
    pub device_lost_reason: Option<String>,
    /// The host rerouted the stream to a different device (for example the OS
    /// default output changed). The stream may still be audible, but the app
    /// should reopen it so settings and device metadata stay in sync.
    #[serde(default)]
    pub device_changed: bool,
    pub dsp: notwig_ui::DspAnalysisView,
    pub(crate) load_ms_history: Vec<f32>,
    pub(crate) jitter_history: Vec<f32>,
}

impl AudioSnapshot {
    /// Meters are only meaningful while transport is running. Stopping or
    /// pausing playback must drop stale peak values instead of leaving the last
    /// active block on screen.
    #[must_use]
    pub fn meters_for_display(&self, playing: bool) -> BTreeMap<TrackId, MeterValues> {
        if self.running && playing {
            self.meters.clone()
        } else {
            BTreeMap::new()
        }
    }

    #[must_use]
    pub fn device_meters_for_display(
        &self,
        playing: bool,
    ) -> BTreeMap<DeviceId, DeviceMeterSnapshot> {
        if self.running && playing {
            self.device_meters.clone()
        } else {
            BTreeMap::new()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DspSample {
    usage_percent: f32,
    load_ms: f32,
    jitter_percent: f32,
    frames: u32,
    output_latency_samples: u64,
}

pub struct AudioService {
    _stream: cpal::Stream,
    input_hub: InputHub,
    pub snapshot: Arc<Mutex<AudioSnapshot>>,
    pub settings: AudioSettings,
}

pub fn settings_view(
    drivers: Vec<notwig_ui::AudioDriverView>,
    settings: &AudioSettings,
    error: Option<String>,
) -> notwig_ui::AudioSettingsView {
    let warning = if settings.device == SYSTEM_OUT_DEVICE_ID {
        Some(SYSTEM_OUT_WARNING.to_string())
    } else {
        None
    };
    notwig_ui::AudioSettingsView {
        drivers,
        current: notwig_ui::AudioSettingsValue {
            driver: settings.driver.clone(),
            device: settings.device.clone(),
            sample_rate: settings
                .sample_rate
                .map_or_else(|| "auto".into(), |rate| rate.to_string()),
            block_size: settings.block_size,
        },
        error,
        warning,
    }
}

impl AudioService {
    pub fn start(control: ControlHandle, project_rate: u32) -> Result<Self, String> {
        Self::start_with(
            control,
            project_rate,
            &AudioSettings {
                sample_rate: Some(project_rate),
                ..AudioSettings::default()
            },
        )
    }

    pub fn start_with(
        control: ControlHandle,
        project_rate: u32,
        settings: &AudioSettings,
    ) -> Result<Self, String> {
        let host = resolve_host(&settings.driver)?;
        let device = resolve_device(&host, &settings.device)?;
        let device_name = device.description().map_or_else(
            |_| "Output".to_string(),
            |description| description.name().to_string(),
        );
        let default = device
            .default_output_config()
            .map_err(|err| format!("output config: {err}"))?;
        let sample_format = default.sample_format();
        let rate = settings.sample_rate.unwrap_or(project_rate);
        let mut config = choose_config(&device, rate).unwrap_or_else(|| {
            tracing::warn!(
                requested = rate,
                fallback = default.sample_rate(),
                "output device does not support the requested sample rate; using its default"
            );
            let config: StreamConfig = default.into();
            config
        });
        config.buffer_size = if settings.block_size > 0 {
            cpal::BufferSize::Fixed(settings.block_size)
        } else {
            cpal::BufferSize::Default
        };
        let deadline_ms = if settings.block_size > 0 {
            settings.block_size as f32 / config.sample_rate as f32 * 1000.0
        } else {
            0.0
        };
        let snapshot = Arc::new(Mutex::new(AudioSnapshot {
            running: false,
            driver: settings.driver.clone(),
            device_name,
            sample_rate: config.sample_rate,
            channels: config.channels,
            block_size: settings.block_size,
            error: None,
            dsp: notwig_ui::DspAnalysisView {
                sample_rate: config.sample_rate,
                block_size: settings.block_size,
                deadline_ms,
                history: Vec::with_capacity(DSP_HISTORY),
                ..notwig_ui::DspAnalysisView::default()
            },
            load_ms_history: Vec::with_capacity(DSP_HISTORY),
            jitter_history: Vec::with_capacity(DSP_HISTORY),
            ..AudioSnapshot::default()
        }));
        let input_hub = InputHub::new(config.sample_rate);
        let mut engine = Engine::new(EngineConfig::new(
            config.sample_rate,
            settings.block_size.max(16) as usize,
        ));
        engine.attach_control(control);
        engine.attach_input_bus(input_hub.bus());
        let stream = match sample_format {
            SampleFormat::F32 => build::<f32>(&device, config, engine, Arc::clone(&snapshot)),
            SampleFormat::I16 => build::<i16>(&device, config, engine, Arc::clone(&snapshot)),
            SampleFormat::U16 => build::<u16>(&device, config, engine, Arc::clone(&snapshot)),
            SampleFormat::I32 => build::<i32>(&device, config, engine, Arc::clone(&snapshot)),
            other => Err(format!("unsupported sample format {other}")),
        }?;
        stream.play().map_err(|err| format!("start audio: {err}"))?;
        if let Ok(mut snapshot) = snapshot.lock() {
            snapshot.running = true;
        }
        Ok(Self {
            _stream: stream,
            input_hub,
            snapshot,
            settings: settings.clone(),
        })
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        self.snapshot.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |snapshot| snapshot.clone(),
        )
    }

    pub(crate) fn configure_inputs(
        &self,
        requests: &[(TrackId, notwig_core::AudioInputRef)],
    ) -> Result<Vec<(TrackId, u16)>, String> {
        let requests: Vec<InputRequest> = requests
            .iter()
            .map(|(track_id, input)| InputRequest {
                track_id: *track_id,
                input: input.clone(),
            })
            .collect();
        self.input_hub.configure(&requests)
    }

    pub(crate) fn begin_recording(
        &self,
        slots: Vec<(TrackId, u16)>,
        sample_rate: u32,
    ) -> RecordingSession {
        RecordingSession::start(self.input_hub.bus(), slots, sample_rate)
    }

    pub fn take_input_error(&self) -> Option<String> {
        self.input_hub.take_error()
    }

    pub fn available_input_options() -> Vec<notwig_ui::AudioInputView> {
        available_input_options()
    }

    pub fn available_options() -> Vec<notwig_ui::AudioDriverView> {
        let mut drivers = Vec::new();
        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            let mut device_views = Vec::new();
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    let Ok(description) = device.description() else {
                        continue;
                    };
                    let name = description.name().to_string();
                    device_views.push(notwig_ui::AudioDeviceView {
                        id: name.clone(),
                        label: name,
                        sample_rates: supported_sample_rates(&device),
                    });
                }
            }
            device_views.push(system_out_device(&host));
            drivers.push(notwig_ui::AudioDriverView {
                id: host_id.name().to_string(),
                label: host_id.name().to_string(),
                devices: device_views,
            });
        }
        drivers
    }
}

fn supported_sample_rates(device: &cpal::Device) -> Vec<u32> {
    let mut rates = std::collections::BTreeSet::new();
    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            rates.insert(range.min_sample_rate());
            rates.insert(range.max_sample_rate());
            for rate in COMMON_SAMPLE_RATES {
                if range.min_sample_rate() <= rate && rate <= range.max_sample_rate() {
                    rates.insert(rate);
                }
            }
        }
    }
    if rates.is_empty()
        && let Ok(default) = device.default_output_config()
    {
        rates.insert(default.sample_rate());
    }
    rates.into_iter().collect()
}

fn system_out_device(host: &cpal::Host) -> notwig_ui::AudioDeviceView {
    let sample_rates = host.default_output_device().map_or_else(
        || COMMON_SAMPLE_RATES.to_vec(),
        |device| {
            let rates = supported_sample_rates(&device);
            if rates.is_empty() {
                COMMON_SAMPLE_RATES.to_vec()
            } else {
                rates
            }
        },
    );
    notwig_ui::AudioDeviceView {
        id: SYSTEM_OUT_DEVICE_ID.to_string(),
        label: SYSTEM_OUT_LABEL.to_string(),
        sample_rates,
    }
}

/// Resolve an output selection to a hardware device description. `SYSTEM_OUT`
/// and the legacy empty string both use the host's default output device.
#[doc(hidden)]
pub fn resolve_output_device_name(driver: &str, name: &str) -> Result<String, String> {
    let host = resolve_host(driver)?;
    let device = resolve_device(&host, name)?;
    device
        .description()
        .map(|description| description.name().to_string())
        .map_err(|err| format!("output device description: {err}"))
}

pub(crate) fn resolve_host(driver: &str) -> Result<cpal::Host, String> {
    if driver.is_empty() {
        return Ok(cpal::default_host());
    }
    let id = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name() == driver)
        .ok_or_else(|| format!("audio driver not found: {driver}"))?;
    cpal::host_from_id(id).map_err(|err| format!("audio driver: {err}"))
}

fn resolve_device(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
    if name.is_empty() || name == SYSTEM_OUT_DEVICE_ID {
        return host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string());
    }
    host.output_devices()
        .map_err(|err| format!("output devices: {err}"))?
        .find(|device| {
            device
                .description()
                .ok()
                .map(|description| description.name().to_string())
                .as_deref()
                == Some(name)
        })
        .ok_or_else(|| format!("audio device not found: {name}"))
}

pub(crate) fn resolve_input_device(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
    if name.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string());
    }
    host.input_devices()
        .map_err(|err| format!("input devices: {err}"))?
        .find(|device| {
            device
                .description()
                .ok()
                .map(|description| description.name().to_string())
                .as_deref()
                == Some(name)
        })
        .ok_or_else(|| format!("input device not found: {name}"))
}

fn choose_config(device: &cpal::Device, project_rate: u32) -> Option<StreamConfig> {
    if let Ok(default) = device.default_output_config() {
        if default.sample_rate() == project_rate && default.channels() >= 1 {
            let mut config: StreamConfig = default.into();
            config.buffer_size = cpal::BufferSize::Default;
            return Some(config);
        }
    }
    let configs = device.supported_output_configs().ok()?;
    let mut candidate = None;
    for range in configs {
        if range.min_sample_rate() <= project_rate
            && project_rate <= range.max_sample_rate()
            && range.channels() >= 1
        {
            let config = range.with_sample_rate(project_rate);
            let mut stream_config: StreamConfig = config.into();
            stream_config.buffer_size = cpal::BufferSize::Default;
            if stream_config.channels >= 2 {
                return Some(stream_config);
            }
            candidate = Some(stream_config);
        }
    }
    candidate
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut engine: Engine,
    snapshot: Arc<Mutex<AudioSnapshot>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    let initial_frames = match config.buffer_size {
        cpal::BufferSize::Fixed(frames) => usize::try_from(frames).unwrap_or(1024),
        cpal::BufferSize::Default => 1024,
    };
    let mut scratch: Vec<f32> = vec![0.0; initial_frames.max(1) * 2];
    let mut events = Vec::with_capacity(4096);
    let mut last_callback: Option<Instant> = None;
    let error_snapshot = Arc::clone(&snapshot);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                if frames == 0 {
                    return;
                }
                if scratch.len() < frames * 2 {
                    scratch.resize(frames * 2, 0.0);
                }
                let mut processed = false;
                let started = Instant::now();
                let period = last_callback.map(|previous| (started - previous).as_secs_f64());
                last_callback = Some(started);
                let crashed = snapshot.try_lock().is_ok_and(|snapshot| snapshot.crashed);
                let outcome = if crashed {
                    None
                } else {
                    Some(catch_unwind(AssertUnwindSafe(|| {
                        engine.process(&mut scratch[..frames * 2], frames)
                    })))
                };
                let processed_ok = matches!(outcome, Some(Ok(Ok(()))));
                if let Some(outcome) = outcome {
                    match outcome {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => {
                            if let Ok(mut snapshot) = snapshot.try_lock() {
                                snapshot.error = Some(err.to_string());
                            }
                        }
                        Err(_) => {
                            if let Ok(mut snapshot) = snapshot.try_lock() {
                                snapshot.crashed = true;
                                snapshot.error =
                                    Some("Audio engine crashed while processing a block.".into());
                            }
                        }
                    }
                }
                if processed_ok {
                    processed = true;
                    let block_seconds = frames as f64 / f64::from(sample_rate.max(1));
                    let elapsed = started.elapsed();
                    let timestamp = info.timestamp();
                    let output_latency_samples = (timestamp
                        .playback
                        .duration_since(timestamp.callback)
                        .as_secs_f64()
                        * f64::from(sample_rate))
                    .round() as u64;
                    let dsp = DspSample {
                        usage_percent: (elapsed.as_secs_f64() / block_seconds * 100.0) as f32,
                        load_ms: elapsed.as_secs_f64() as f32 * 1000.0,
                        jitter_percent: period.map_or(0.0, |period| {
                            ((period - block_seconds).abs() / block_seconds * 100.0) as f32
                        }),
                        frames: frames as u32,
                        output_latency_samples,
                    };
                    events.clear();
                    engine.drain_events(&mut events);
                    update_snapshot(&snapshot, &engine, &events, dsp);
                }
                for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
                    let left = if processed {
                        scratch[frame_index * 2]
                    } else {
                        0.0
                    };
                    let right = if processed {
                        scratch[frame_index * 2 + 1]
                    } else {
                        0.0
                    };
                    for (channel_index, sample) in frame.iter_mut().enumerate() {
                        let value = match channel_index {
                            0 => left,
                            1 => right,
                            _ => 0.0,
                        };
                        *sample = T::from_sample(value);
                    }
                }
            },
            move |err| {
                if let Ok(mut snapshot) = error_snapshot.lock() {
                    let kind = err.kind();
                    if is_device_loss_error(kind) {
                        snapshot.running = false;
                        snapshot.playing = false;
                        snapshot.device_lost = true;
                        snapshot.device_lost_reason = Some(err.to_string());
                        snapshot.error = None;
                        snapshot.meters.clear();
                        snapshot.device_meters.clear();
                    } else if is_device_change_error(kind) {
                        snapshot.device_changed = true;
                    } else {
                        snapshot.error = Some(err.to_string());
                    }
                }
            },
            None,
        )
        .map_err(|err| format!("build audio stream: {err}"))
}

fn update_snapshot(
    snapshot: &Arc<Mutex<AudioSnapshot>>,
    engine: &Engine,
    events: &[RtEvent],
    dsp: DspSample,
) {
    let Ok(mut snapshot) = snapshot.try_lock() else {
        return;
    };
    snapshot.playhead_samples = engine.playhead_samples();
    snapshot.playhead_updated = Some(Instant::now());
    snapshot.playing = engine.transport().playing;
    if !snapshot.playing {
        snapshot.meters.clear();
    }
    snapshot.device_meters.clear();
    if snapshot.playing {
        for (device_id, meter) in engine.device_meter_snapshot() {
            snapshot.device_meters.insert(
                device_id,
                DeviceMeterSnapshot {
                    input_peak_l: meter.input_peak_l,
                    input_peak_r: meter.input_peak_r,
                    output_peak_l: meter.output_peak_l,
                    output_peak_r: meter.output_peak_r,
                },
            );
        }
    }
    snapshot.cpu_load = Some(dsp.usage_percent);
    snapshot.block_size = dsp.frames;
    snapshot.dsp.sample_rate = snapshot.sample_rate;

    snapshot.load_ms_history.push(dsp.load_ms);
    if snapshot.load_ms_history.len() > DSP_HISTORY {
        snapshot.load_ms_history.remove(0);
    }
    snapshot.jitter_history.push(dsp.jitter_percent);
    if snapshot.jitter_history.len() > DSP_HISTORY {
        snapshot.jitter_history.remove(0);
    }
    let load_avg =
        snapshot.load_ms_history.iter().sum::<f32>() / snapshot.load_ms_history.len().max(1) as f32;
    let load_max = snapshot
        .load_ms_history
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    let jitter_max = snapshot
        .jitter_history
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);

    snapshot.dsp.history.push(dsp.usage_percent);
    if snapshot.dsp.history.len() > DSP_HISTORY {
        snapshot.dsp.history.remove(0);
    }
    let rate = snapshot.dsp.sample_rate.max(1) as f32;
    snapshot.dsp.block_size = dsp.frames;
    snapshot.dsp.deadline_ms = dsp.frames as f32 / rate * 1000.0;
    snapshot.dsp.load_avg_ms = load_avg;
    snapshot.dsp.load_max_ms = load_max;
    snapshot.dsp.period_jitter_percent = jitter_max;
    snapshot.dsp.output_latency_samples = dsp.output_latency_samples;
    snapshot.dsp.input_latency_samples = 0;
    snapshot.dsp.timing = "Device".into();
    for event in events {
        match event {
            RtEvent::Meters {
                track_id,
                peak_l,
                peak_r,
                rms_l,
                rms_r,
            } => {
                snapshot.meters.insert(
                    *track_id,
                    MeterValues {
                        peak_l: *peak_l,
                        peak_r: *peak_r,
                        rms_l: *rms_l,
                        rms_r: *rms_r,
                    },
                );
            }
            RtEvent::Status { xruns, .. } => {
                snapshot.xruns = *xruns;
            }
            RtEvent::Error { message, .. } => {
                snapshot.error = Some(message.clone());
            }
            _ => {}
        }
    }
}
