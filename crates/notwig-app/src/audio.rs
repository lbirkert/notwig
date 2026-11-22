use crate::clock::Instant;
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use crossbeam_queue::ArrayQueue;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::{DeviceId, RtEvent, TrackId};
use notwig_engine::{ControlHandle, Engine, EngineConfig};
use notwig_ext_api::DeviceMeterSnapshot;
use notwig_ui::MeterValues;

use crate::audio_input::{InputHub, InputRequest, available_input_options};
#[cfg(not(target_arch = "wasm32"))]
use crate::audio_worker::RenderPipeline;
use crate::recording::RecordingSession;

/// Synthetic device id that always resolves to the host's default output
/// device. It is selectable in Settings instead of relying on an empty device
/// string to imply the system default.
pub const SYSTEM_OUT_DEVICE_ID: &str = "system-out";

const SYSTEM_OUT_LABEL: &str = "System Out";
const SYSTEM_OUT_WARNING: &str = "System Out follows the operating system's default output device.";
const COMMON_SAMPLE_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
/// Default device callback block used for the low-latency output path. The
/// project is rendered in larger chunks and streamed into this small device
/// buffer.
pub(crate) const DEFAULT_LOW_LATENCY_BLOCK_SIZE: u32 = 16;

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioSettings {
    pub driver: String,
    pub device: String,
    pub sample_rate: Option<u32>,
    pub block_size: u32,
    #[serde(default = "default_low_latency_block_size")]
    pub low_latency_block_size: u32,
    #[serde(default = "default_low_latency_monitoring")]
    pub low_latency_monitoring: bool,
    #[serde(default)]
    pub channels: Vec<notwig_ui::AudioChannelConfig>,
    #[serde(default)]
    pub master_output: Option<String>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            driver: default_driver_name(),
            device: SYSTEM_OUT_DEVICE_ID.into(),
            sample_rate: None,
            block_size: 0,
            low_latency_block_size: DEFAULT_LOW_LATENCY_BLOCK_SIZE,
            low_latency_monitoring: true,
            channels: Vec::new(),
            master_output: None,
        }
    }
}

const fn default_low_latency_block_size() -> u32 {
    DEFAULT_LOW_LATENCY_BLOCK_SIZE
}

const fn default_low_latency_monitoring() -> bool {
    true
}

#[must_use]
pub(crate) fn default_driver_name() -> String {
    cpal::default_host().id().name().to_string()
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
pub(crate) struct DspSample {
    pub(crate) usage_percent: f32,
    pub(crate) load_ms: f32,
    pub(crate) jitter_percent: f32,
    pub(crate) frames: u32,
    pub(crate) output_latency_samples: u64,
}

pub struct AudioService {
    _stream: cpal::Stream,
    input_hub: InputHub,
    pub snapshot: Arc<Mutex<AudioSnapshot>>,
    pub settings: AudioSettings,
    #[cfg(not(target_arch = "wasm32"))]
    _render: RenderPipeline,
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
            low_latency_block_size: settings.low_latency_block_size,
            low_latency_monitoring: settings.low_latency_monitoring,
            channels: settings.channels.clone(),
            master_output: settings.master_output.clone(),
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
        let project_frames = if settings.low_latency_monitoring {
            settings.block_size
        } else {
            settings.low_latency_block_size
        };
        let project_frames = usize::try_from(project_frames.max(16)).unwrap_or(512);
        let device_block = if settings.low_latency_monitoring {
            settings
                .low_latency_block_size
                .max(1)
                .min(u32::try_from(project_frames).unwrap_or(u32::MAX))
        } else {
            u32::try_from(project_frames).unwrap_or(u32::MAX)
        };
        config.buffer_size = cpal::BufferSize::Fixed(device_block);
        let deadline_ms = device_block as f32 / config.sample_rate as f32 * 1000.0;
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
        let input_hub = InputHub::new(config.sample_rate, device_block);
        let output_route = output_route(settings, config.channels);
        let high_latency_frames = if settings.low_latency_monitoring {
            project_frames
        } else {
            0
        };
        let mut engine = Engine::new(EngineConfig::new(config.sample_rate, project_frames));
        engine.attach_control(control);
        engine.attach_input_bus(input_hub.bus());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let device_frames = usize::try_from(device_block).unwrap_or(1);
            let pipeline = RenderPipeline::start(
                engine,
                Arc::clone(&snapshot),
                config.sample_rate,
                device_frames,
                project_frames,
                high_latency_frames,
            );
            let high = Arc::clone(&pipeline.high);
            let low = Arc::clone(&pipeline.low);
            let ready = Arc::clone(&pipeline.ready);
            let output_latency_samples = Arc::clone(&pipeline.output_latency_samples);
            let error_snapshot = Arc::clone(&snapshot);
            let stream = match sample_format {
                SampleFormat::F32 => build_rendered::<f32>(
                    &device,
                    config,
                    high,
                    low,
                    ready,
                    output_latency_samples,
                    error_snapshot,
                    output_route,
                ),
                SampleFormat::I16 => build_rendered::<i16>(
                    &device,
                    config,
                    high,
                    low,
                    ready,
                    output_latency_samples,
                    error_snapshot,
                    output_route,
                ),
                SampleFormat::U16 => build_rendered::<u16>(
                    &device,
                    config,
                    high,
                    low,
                    ready,
                    output_latency_samples,
                    error_snapshot,
                    output_route,
                ),
                SampleFormat::I32 => build_rendered::<i32>(
                    &device,
                    config,
                    high,
                    low,
                    ready,
                    output_latency_samples,
                    error_snapshot,
                    output_route,
                ),
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
                _render: pipeline,
            })
        }

        #[cfg(target_arch = "wasm32")]
        {
            let stream = match sample_format {
                SampleFormat::F32 => build::<f32>(
                    &device,
                    config,
                    engine,
                    Arc::clone(&snapshot),
                    output_route,
                    project_frames,
                    high_latency_frames,
                ),
                SampleFormat::I16 => build::<i16>(
                    &device,
                    config,
                    engine,
                    Arc::clone(&snapshot),
                    output_route,
                    project_frames,
                    high_latency_frames,
                ),
                SampleFormat::U16 => build::<u16>(
                    &device,
                    config,
                    engine,
                    Arc::clone(&snapshot),
                    output_route,
                    project_frames,
                    high_latency_frames,
                ),
                SampleFormat::I32 => build::<i32>(
                    &device,
                    config,
                    engine,
                    Arc::clone(&snapshot),
                    output_route,
                    project_frames,
                    high_latency_frames,
                ),
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
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        let mut snapshot = self.snapshot.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |snapshot| snapshot.clone(),
        );
        snapshot.dsp.input_latency_samples = self.input_hub.bus().input_latency_samples();
        snapshot
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

    pub fn prepare_input_options() {
        #[cfg(target_arch = "wasm32")]
        crate::audio_input::prepare_input_options();
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
                        input_channels: input_channel_count(&device),
                        output_channels: output_channel_count(&device),
                        sample_rates: supported_sample_rates(&device),
                    });
                }
            }
            device_views.push(system_out_device(&host));
            let label = if cfg!(target_arch = "wasm32") && host_id.name() == "WebAudio" {
                "Web Audio".to_string()
            } else {
                host_id.name().to_string()
            };
            drivers.push(notwig_ui::AudioDriverView {
                id: host_id.name().to_string(),
                label,
                devices: device_views,
            });
        }
        #[cfg(target_arch = "wasm32")]
        if drivers.is_empty() {
            // Some browsers do not expose the WebAudio host until an
            // AudioContext exists, but the host itself still works. Keep a
            // synthetic entry so the settings page can open the driver.
            drivers.push(notwig_ui::AudioDriverView {
                id: "WebAudio".into(),
                label: "Web Audio".into(),
                devices: vec![notwig_ui::AudioDeviceView {
                    id: "Default Device".into(),
                    label: "Default Output".into(),
                    input_channels: 2,
                    output_channels: 2,
                    sample_rates: COMMON_SAMPLE_RATES.to_vec(),
                }],
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let web_inputs = crate::audio_input::available_input_options();
            let web_input_channels = web_inputs
                .iter()
                .filter(|input| input.driver == "WebAudio")
                .map(|input| input.channels)
                .max()
                .unwrap_or(0);
            for driver in &mut drivers {
                if driver.id == "WebAudio" {
                    for device in &mut driver.devices {
                        device.input_channels = device.input_channels.max(web_input_channels);
                    }
                }
            }
        }
        drivers
    }
}

fn input_channel_count(device: &cpal::Device) -> u16 {
    device.supported_input_configs().map_or(0, |configs| {
        configs.map(|config| config.channels()).max().unwrap_or(0)
    })
}

fn output_channel_count(device: &cpal::Device) -> u16 {
    device
        .supported_output_configs()
        .map_or(2, |configs| {
            configs.map(|config| config.channels()).max().unwrap_or(2)
        })
        .max(1)
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
    let (output_channels, sample_rates) = host.default_output_device().map_or_else(
        || (2, COMMON_SAMPLE_RATES.to_vec()),
        |device| {
            let rates = supported_sample_rates(&device);
            let rates = if rates.is_empty() {
                COMMON_SAMPLE_RATES.to_vec()
            } else {
                rates
            };
            (output_channel_count(&device), rates)
        },
    );
    let input_channels = host
        .default_input_device()
        .map_or(0, |device| input_channel_count(&device));
    notwig_ui::AudioDeviceView {
        id: SYSTEM_OUT_DEVICE_ID.to_string(),
        label: SYSTEM_OUT_LABEL.to_string(),
        input_channels,
        output_channels,
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn resolve_input_device(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
    if name.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string());
    }
    if name == SYSTEM_OUT_DEVICE_ID {
        if let Some(device) = host.default_output_device()
            && device_has_input(&device)
        {
            return Ok(device);
        }
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

#[cfg(not(target_arch = "wasm32"))]
fn device_has_input(device: &cpal::Device) -> bool {
    device
        .supported_input_configs()
        .is_ok_and(|mut configs| configs.next().is_some())
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

fn output_route(settings: &AudioSettings, stream_channels: u16) -> (usize, Option<usize>) {
    let default_left = 0;
    let default_right = (stream_channels >= 2).then_some(1);
    let Some(id) = settings.master_output.as_deref() else {
        return (default_left, default_right);
    };
    let Some(channel) = settings.channels.iter().find(|channel| {
        channel.id == id
            && channel.direction == notwig_ui::AudioChannelDirection::Output
            && channel.driver == settings.driver
            && channel.device == settings.device
    }) else {
        return (default_left, default_right);
    };
    let left = usize::from(channel.left_channel);
    if left >= usize::from(stream_channels.max(1)) {
        return (default_left, default_right);
    }
    if channel.channel_count() < 2 {
        return (left, None);
    }
    let Some(right) = channel
        .resolved_right_channel()
        .map(usize::from)
        .filter(|right| *right < usize::from(stream_channels) && *right != left)
    else {
        return (default_left, default_right);
    };
    (left, Some(right))
}

#[cfg(not(target_arch = "wasm32"))]
fn build_rendered<T>(
    device: &cpal::Device,
    config: StreamConfig,
    high: Arc<ArrayQueue<f32>>,
    low: Arc<ArrayQueue<f32>>,
    ready: Arc<AtomicBool>,
    output_latency_samples: Arc<AtomicU64>,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    let error_snapshot = Arc::clone(&snapshot);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                if frames == 0 {
                    return;
                }
                let timestamp = info.timestamp();
                let latency = (timestamp
                    .playback
                    .duration_since(timestamp.callback)
                    .as_secs_f64()
                    * f64::from(sample_rate))
                .round() as u64;
                output_latency_samples.store(latency, Ordering::Relaxed);
                let high_ready = ready.load(Ordering::Acquire);
                for frame in data.chunks_mut(channels) {
                    let low_left = low.pop().unwrap_or(0.0);
                    let low_right = low.pop().unwrap_or(0.0);
                    let high_left = if high_ready {
                        high.pop().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    let high_right = if high_ready {
                        high.pop().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    let left = high_left + low_left;
                    let right = high_right + low_right;
                    for (channel_index, sample) in frame.iter_mut().enumerate() {
                        let value = if channel_index == output_route.0 {
                            left
                        } else if Some(channel_index) == output_route.1 {
                            right
                        } else {
                            0.0
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

#[cfg(target_arch = "wasm32")]
fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut engine: Engine,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
    project_frames: usize,
    high_latency_frames: usize,
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
    let project_frames = project_frames.max(initial_frames.max(1));
    let mut low_scratch: Vec<f32> = vec![0.0; initial_frames.max(1) * 2];
    let mut project_scratch: Vec<f32> = vec![0.0; project_frames * 2];
    // Worst case is the fill target plus one full project chunk of overshoot.
    // Since device_frames <= project_frames, 6 * project_frames covers it.
    let mut project_ring =
        std::collections::VecDeque::with_capacity(project_frames.saturating_mul(6));
    let mut project_generation = engine.transport_generation();
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
                if low_scratch.len() < frames * 2 {
                    low_scratch.resize(frames * 2, 0.0);
                }
                let mut processed = false;
                let started = Instant::now();
                let period = last_callback.map(|previous| (started - previous).as_secs_f64());
                last_callback = Some(started);
                let crashed = snapshot.try_lock().is_ok_and(|snapshot| snapshot.crashed);
                if !crashed {
                    let mut crashed_now = false;
                    let target_len = frames.saturating_add(high_latency_frames) * 2;
                    while project_ring.len() < target_len {
                        let outcome = catch_unwind(AssertUnwindSafe(|| {
                            engine
                                .process(&mut project_scratch[..project_frames * 2], project_frames)
                        }));
                        match outcome {
                            Ok(Ok(())) => {
                                let generation = engine.transport_generation();
                                if generation != project_generation {
                                    project_ring.clear();
                                    project_generation = generation;
                                }
                                project_ring
                                    .extend(project_scratch[..project_frames * 2].iter().copied());
                            }
                            Ok(Err(err)) => {
                                project_ring.clear();
                                if let Ok(mut snapshot) = snapshot.try_lock() {
                                    snapshot.error = Some(err.to_string());
                                }
                                break;
                            }
                            Err(_) => {
                                project_ring.clear();
                                crashed_now = true;
                                if let Ok(mut snapshot) = snapshot.try_lock() {
                                    snapshot.crashed = true;
                                    snapshot.error = Some(
                                        "Audio engine crashed while processing a block.".into(),
                                    );
                                }
                                break;
                            }
                        }
                    }
                    if !crashed_now {
                        low_scratch[..frames * 2].fill(0.0);
                        let outcome = catch_unwind(AssertUnwindSafe(|| {
                            engine
                                .process_low_latency_monitor(&mut low_scratch[..frames * 2], frames)
                        }));
                        if outcome.is_err() {
                            low_scratch[..frames * 2].fill(0.0);
                            if let Ok(mut snapshot) = snapshot.try_lock() {
                                snapshot.crashed = true;
                                snapshot.error =
                                    Some("Audio engine crashed while monitoring input.".into());
                            }
                        }
                    }
                    processed = !crashed_now;
                }
                if processed {
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
                        project_ring.pop_front().unwrap_or(0.0)
                            + low_scratch.get(frame_index * 2).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    let right = if processed {
                        project_ring.pop_front().unwrap_or(0.0)
                            + low_scratch.get(frame_index * 2 + 1).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    for (channel_index, sample) in frame.iter_mut().enumerate() {
                        let value = if channel_index == output_route.0 {
                            left
                        } else if Some(channel_index) == output_route.1 {
                            right
                        } else {
                            0.0
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

#[cfg(target_arch = "wasm32")]
pub(crate) fn update_snapshot(
    snapshot: &Arc<Mutex<AudioSnapshot>>,
    engine: &Engine,
    events: &[RtEvent],
    dsp: DspSample,
) {
    update_snapshot_at(snapshot, engine, events, engine.playhead_samples(), dsp);
}

pub(crate) fn update_snapshot_at(
    snapshot: &Arc<Mutex<AudioSnapshot>>,
    engine: &Engine,
    events: &[RtEvent],
    playhead_samples: u64,
    dsp: DspSample,
) {
    let Ok(mut snapshot) = snapshot.try_lock() else {
        return;
    };
    snapshot.playhead_samples = playhead_samples;
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
