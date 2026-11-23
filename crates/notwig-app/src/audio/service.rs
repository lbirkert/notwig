use std::sync::{Arc, Mutex};

use cpal::StreamConfig;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use notwig_core::TrackId;
use notwig_engine::{ControlHandle, Engine, EngineConfig};

use crate::audio_input::{InputHub, InputRequest, available_input_options};
#[cfg(not(target_arch = "wasm32"))]
use crate::audio_worker::RenderPipeline;
use crate::recording::RecordingSession;

#[cfg(target_arch = "wasm32")]
use super::settings::COMMON_SAMPLE_RATES;
use super::settings::{
    AudioSettings, choose_config, input_channel_count, output_channel_count, output_route,
    resolve_device, resolve_host, supported_sample_rates, system_out_device,
};
use super::snapshot::{AudioSnapshot, DSP_HISTORY};
use super::stream::build_inline_for_format;
#[cfg(not(target_arch = "wasm32"))]
use super::stream::build_rendered_for_format;

pub struct AudioService {
    _stream: cpal::Stream,
    input_hub: InputHub,
    pub snapshot: Arc<Mutex<AudioSnapshot>>,
    pub settings: AudioSettings,
    #[cfg(not(target_arch = "wasm32"))]
    _render: Option<RenderPipeline>,
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
        let use_worker = settings.low_latency_monitoring && device_block < project_frames as u32;

        #[cfg(not(target_arch = "wasm32"))]
        if use_worker {
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
            let stream = build_rendered_for_format(
                &device,
                config,
                sample_format,
                high,
                low,
                ready,
                output_latency_samples,
                error_snapshot,
                output_route,
            )?;
            stream.play().map_err(|err| format!("start audio: {err}"))?;
            if let Ok(mut snapshot) = snapshot.lock() {
                snapshot.running = true;
            }
            return Ok(Self {
                _stream: stream,
                input_hub,
                snapshot,
                settings: settings.clone(),
                _render: Some(pipeline),
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let stream = build_inline_for_format(
                &device,
                config,
                sample_format,
                engine,
                Arc::clone(&snapshot),
                output_route,
                project_frames,
                0,
            )?;
            stream.play().map_err(|err| format!("start audio: {err}"))?;
            if let Ok(mut snapshot) = snapshot.lock() {
                snapshot.running = true;
            }
            Ok(Self {
                _stream: stream,
                input_hub,
                snapshot,
                settings: settings.clone(),
                _render: None,
            })
        }

        #[cfg(target_arch = "wasm32")]
        {
            let stream = build_inline_for_format(
                &device,
                config,
                sample_format,
                engine,
                Arc::clone(&snapshot),
                output_route,
                project_frames,
                high_latency_frames,
            )?;
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
