use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::{RtEvent, TrackId};
use notwig_engine::Engine;
use notwig_ui::MeterValues;

#[derive(Debug, Clone, Default)]
pub struct AudioSnapshot {
    pub running: bool,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub playhead_samples: u64,
    pub playing: bool,
    pub meters: BTreeMap<TrackId, MeterValues>,
    pub xruns: u64,
    pub cpu_load: Option<f32>,
    pub error: Option<String>,
}

pub struct AudioService {
    _stream: cpal::Stream,
    pub snapshot: Arc<Mutex<AudioSnapshot>>,
}

impl AudioService {
    pub fn start(engine: Arc<Mutex<Engine>>, project_rate: u32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string())?;
        let device_name = device.description().map_or_else(
            |_| "Output".into(),
            |description| description.name().to_string(),
        );
        let default = device
            .default_output_config()
            .map_err(|err| format!("output config: {err}"))?;
        let sample_format = default.sample_format();
        let config = choose_config(&device, project_rate).ok_or_else(|| {
            format!(
                "output device does not support {project_rate} Hz; project playback is unavailable"
            )
        })?;
        let snapshot = Arc::new(Mutex::new(AudioSnapshot {
            running: false,
            device_name,
            sample_rate: config.sample_rate,
            channels: config.channels,
            error: None,
            ..AudioSnapshot::default()
        }));
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
        })
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        self.snapshot
            .try_lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }
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
    engine: Arc<Mutex<Engine>>,
    snapshot: Arc<Mutex<AudioSnapshot>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    let mut scratch: Vec<f32> = Vec::new();
    let error_snapshot = Arc::clone(&snapshot);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels;
                if frames == 0 {
                    return;
                }
                scratch.resize(frames * 2, 0.0);
                let mut processed = false;
                let started = Instant::now();
                if let Ok(mut engine) = engine.try_lock() {
                    if engine.process(&mut scratch[..frames * 2], frames).is_ok() {
                        processed = true;
                        let block_seconds = frames as f64 / f64::from(sample_rate.max(1));
                        let load = (started.elapsed().as_secs_f64() / block_seconds) as f32;
                        let mut events = Vec::new();
                        engine.drain_events(&mut events);
                        update_snapshot(&snapshot, &engine, &events, Some(load * 100.0));
                    }
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
    cpu_load: Option<f32>,
) {
    let Ok(mut snapshot) = snapshot.try_lock() else {
        return;
    };
    snapshot.playhead_samples = engine.playhead_samples();
    snapshot.playing = engine.transport().playing;
    snapshot.cpu_load = cpu_load;
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
