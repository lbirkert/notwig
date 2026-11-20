use crate::clock::Instant;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::{RtEvent, TrackId};
use notwig_engine::{ControlHandle, Engine, EngineConfig};
use notwig_ui::MeterValues;

#[derive(Debug, Clone, Default)]
pub struct AudioSettings {
    pub driver: String,
    pub device: String,
    pub sample_rate: Option<u32>,
    pub block_size: u32,
}

const DSP_HISTORY: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct AudioSnapshot {
    pub running: bool,
    pub driver: String,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub block_size: u32,
    pub playhead_samples: u64,
    pub playhead_updated: Option<Instant>,
    pub playing: bool,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub error: Option<String>,
    pub crashed: bool,
    pub dsp: notwig_ui::DspAnalysisView,
    load_ms_history: Vec<f32>,
    jitter_history: Vec<f32>,
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
    pub snapshot: Arc<Mutex<AudioSnapshot>>,
    pub settings: AudioSettings,
}

pub fn settings_view(
    drivers: Vec<notwig_ui::AudioDriverView>,
    settings: &AudioSettings,
    error: Option<String>,
) -> notwig_ui::AudioSettingsView {
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
        warning: None,
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
        let mut config = choose_config(&device, rate).ok_or_else(|| {
            format!("output device does not support {rate} Hz; playback is unavailable")
        })?;
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
        let mut engine = Engine::new(EngineConfig::new(
            config.sample_rate,
            settings.block_size.max(16) as usize,
        ));
        engine.attach_control(control);
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

    pub fn available_options() -> Vec<notwig_ui::AudioDriverView> {
        let mut drivers = Vec::new();
        for host_id in cpal::available_hosts() {
            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            let Ok(devices) = host.output_devices() else {
                continue;
            };
            let mut device_views = Vec::new();
            for device in devices {
                let Ok(description) = device.description() else {
                    continue;
                };
                let name = description.name().to_string();
                let mut rates = std::collections::BTreeSet::new();
                if let Ok(configs) = device.supported_output_configs() {
                    for range in configs {
                        rates.insert(range.min_sample_rate());
                        rates.insert(range.max_sample_rate());
                        for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
                            if range.min_sample_rate() <= rate && rate <= range.max_sample_rate() {
                                rates.insert(rate);
                            }
                        }
                    }
                }
                device_views.push(notwig_ui::AudioDeviceView {
                    id: name.clone(),
                    label: name,
                    sample_rates: rates.into_iter().collect(),
                });
            }
            drivers.push(notwig_ui::AudioDriverView {
                id: host_id.name().to_string(),
                label: host_id.name().to_string(),
                devices: device_views,
            });
        }
        drivers
    }
}

fn resolve_host(driver: &str) -> Result<cpal::Host, String> {
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
    if name.is_empty() {
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
                    snapshot.error = Some(err.to_string());
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
