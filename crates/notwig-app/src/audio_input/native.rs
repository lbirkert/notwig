use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::TrackId;
use notwig_engine::{InputBus, MAX_INPUT_TRACKS};

use super::InputRequest;
use crate::audio::{resolve_host, resolve_input_device};

type DeviceTargets = BTreeMap<(String, String), Vec<(TrackId, u16, u16, Option<u16>)>>;

pub(crate) struct InputHub {
    bus: Arc<InputBus>,
    sample_rate: u32,
    block_size: u32,
    streams: Mutex<BTreeMap<(String, String), cpal::Stream>>,
    error: Arc<Mutex<Option<String>>>,
}

impl InputHub {
    pub(crate) fn new(sample_rate: u32, block_size: u32) -> Self {
        Self {
            bus: Arc::new(InputBus::new(sample_rate)),
            sample_rate,
            block_size: block_size.max(1),
            streams: Mutex::new(BTreeMap::new()),
            error: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn bus(&self) -> Arc<InputBus> {
        Arc::clone(&self.bus)
    }

    pub(crate) fn configure(
        &self,
        requests: &[InputRequest],
    ) -> Result<Vec<(TrackId, u16)>, String> {
        if requests.len() > MAX_INPUT_TRACKS {
            return Err(format!(
                "at most {MAX_INPUT_TRACKS} tracks can be armed for recording"
            ));
        }
        let mut by_device: DeviceTargets = BTreeMap::new();
        let mut routes = Vec::with_capacity(requests.len());
        for (index, request) in requests.iter().enumerate() {
            let slot = u16::try_from(index).map_err(|_| "too many recording tracks".to_string())?;
            let input = &request.input;
            routes.push((request.track_id, slot));
            by_device
                .entry((input.driver.clone(), input.device.clone()))
                .or_default()
                .push((
                    request.track_id,
                    slot,
                    input.channel,
                    (input.channels >= 2).then(|| input.right()),
                ));
        }

        let mut streams = self
            .streams
            .lock()
            .map_err(|_| "input stream lock poisoned".to_string())?;
        streams.clear();
        if let Ok(mut error) = self.error.lock() {
            *error = None;
        }
        for ((driver, device), targets) in by_device {
            let stream = self.open_stream(&driver, &device, &targets)?;
            stream
                .play()
                .map_err(|error| format!("start input stream: {error}"))?;
            streams.insert((driver, device), stream);
        }
        Ok(routes)
    }

    pub(crate) fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut error| error.take())
    }

    fn open_stream(
        &self,
        driver: &str,
        device_name: &str,
        targets: &[(TrackId, u16, u16, Option<u16>)],
    ) -> Result<cpal::Stream, String> {
        let host = resolve_host(driver)?;
        let device = resolve_input_device(&host, device_name)?;
        let max_channel = targets
            .iter()
            .map(|(_, _, left, right)| usize::from((*left).max(right.unwrap_or(*left))))
            .max()
            .unwrap_or(0);
        let (mut config, sample_format) =
            choose_input_config(&device, self.sample_rate, max_channel)
                .ok_or_else(|| format!("input device does not support {} Hz", self.sample_rate))?;
        config.buffer_size = cpal::BufferSize::Fixed(self.block_size);
        let targets: Vec<(u16, u16, Option<u16>)> = targets
            .iter()
            .map(|(_, slot, left, right)| (*slot, *left, *right))
            .collect();
        let bus = Arc::clone(&self.bus);
        let error = Arc::clone(&self.error);
        match sample_format {
            SampleFormat::F32 => build_input::<f32>(&device, config, bus, targets, error),
            SampleFormat::I16 => build_input::<i16>(&device, config, bus, targets, error),
            SampleFormat::U16 => build_input::<u16>(&device, config, bus, targets, error),
            SampleFormat::I32 => build_input::<i32>(&device, config, bus, targets, error),
            SampleFormat::I8 => build_input::<i8>(&device, config, bus, targets, error),
            SampleFormat::U8 => build_input::<u8>(&device, config, bus, targets, error),
            other => Err(format!("unsupported input sample format {other}")),
        }
    }
}

fn choose_input_config(
    device: &cpal::Device,
    sample_rate: u32,
    max_channel: usize,
) -> Option<(StreamConfig, SampleFormat)> {
    if let Ok(default) = device.default_input_config()
        && default.sample_rate() == sample_rate
        && usize::from(default.channels()) > max_channel
    {
        return Some((default.config(), default.sample_format()));
    }
    let mut candidate = None;
    for range in device.supported_input_configs().ok()? {
        if range.min_sample_rate() <= sample_rate
            && sample_rate <= range.max_sample_rate()
            && usize::from(range.channels()) > max_channel
        {
            let config: StreamConfig = range.with_sample_rate(sample_rate).into();
            if config.channels as usize > max_channel {
                candidate = Some((config, range.sample_format()));
            }
        }
    }
    candidate
}

fn build_input<T>(
    device: &cpal::Device,
    config: StreamConfig,
    bus: Arc<InputBus>,
    targets: Vec<(u16, u16, Option<u16>)>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    let error_callback = Arc::clone(&error);
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if targets.is_empty() {
                    return;
                }
                let frames = data.len() / channels;
                for frame in 0..frames {
                    for (slot, left_channel, right_channel) in &targets {
                        let left_index = frame * channels + usize::from(*left_channel);
                        let Some(left) = data.get(left_index) else {
                            continue;
                        };
                        if let Some(right_channel) = right_channel {
                            let right_index = frame * channels + usize::from(*right_channel);
                            let right = data.get(right_index).copied().unwrap_or(*left);
                            bus.push_stereo(
                                *slot,
                                f32::from_sample(*left),
                                f32::from_sample(right),
                            );
                        } else {
                            bus.push(*slot, f32::from_sample(*left));
                        }
                    }
                }
            },
            move |err| {
                if let Ok(mut error) = error_callback.lock() {
                    *error = Some(format!("input stream error: {err}"));
                }
            },
            None,
        )
        .map_err(|error| format!("build input stream: {error}"))
}

pub(crate) fn available_input_options() -> Vec<notwig_ui::AudioInputView> {
    let mut inputs = Vec::new();
    for host_id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        let Ok(devices) = host.input_devices() else {
            continue;
        };
        for device in devices {
            let Ok(description) = device.description() else {
                continue;
            };
            let name = description.name().to_string();
            let channels = device
                .supported_input_configs()
                .map(|configs| {
                    configs
                        .map(|config| config.channels())
                        .max()
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            if channels == 0 {
                continue;
            }
            inputs.push(notwig_ui::AudioInputView {
                driver: host_id.name().to_string(),
                device: name.clone(),
                label: name,
                channels,
            });
        }
    }
    inputs
}
