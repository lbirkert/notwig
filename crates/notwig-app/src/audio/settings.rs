use cpal::StreamConfig;
use cpal::traits::{DeviceTrait, HostTrait};

/// Synthetic device id that always resolves to the host's default output
/// device. It is selectable in Settings instead of relying on an empty device
/// string to imply the system default.
pub const SYSTEM_OUT_DEVICE_ID: &str = "system-out";

const SYSTEM_OUT_LABEL: &str = "System Out";
const SYSTEM_OUT_WARNING: &str = "System Out follows the operating system's default output device. If that output has no inputs, recording uses the default input device on a separate clock.";
pub(super) const COMMON_SAMPLE_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
/// Default device callback block used for the low-latency output path. The
/// project can still render in larger chunks when low-latency monitoring is
/// enabled, but the device buffer itself starts at a safe, stable 512 samples.
pub(crate) const DEFAULT_LOW_LATENCY_BLOCK_SIZE: u32 = 512;

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

pub(super) fn input_channel_count(device: &cpal::Device) -> u16 {
    device.supported_input_configs().map_or(0, |configs| {
        configs.map(|config| config.channels()).max().unwrap_or(0)
    })
}

pub(super) fn output_channel_count(device: &cpal::Device) -> u16 {
    device
        .supported_output_configs()
        .map_or(2, |configs| {
            configs.map(|config| config.channels()).max().unwrap_or(2)
        })
        .max(1)
}

pub(super) fn supported_sample_rates(device: &cpal::Device) -> Vec<u32> {
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

pub(super) fn system_out_device(host: &cpal::Host) -> notwig_ui::AudioDeviceView {
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

pub(crate) fn resolve_device(host: &cpal::Host, name: &str) -> Result<cpal::Device, String> {
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

pub(super) fn choose_config(device: &cpal::Device, project_rate: u32) -> Option<StreamConfig> {
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

pub(super) fn output_route(
    settings: &AudioSettings,
    stream_channels: u16,
) -> (usize, Option<usize>) {
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
