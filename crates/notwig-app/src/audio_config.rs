#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::AudioSettings;

const CONFIG_VERSION: u32 = 2;
const DEFAULT_BLOCK_SIZE: u32 = 512;
const DEFAULT_LOW_LATENCY_BLOCK_SIZE: u32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedAudioSettings {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    driver: String,
    #[serde(default)]
    device: String,
    #[serde(default = "default_block_size")]
    block_size: u32,
    #[serde(default = "default_low_latency_block_size")]
    low_latency_block_size: u32,
    #[serde(default)]
    channels: Vec<notwig_ui::AudioChannelConfig>,
    #[serde(default)]
    master_output: Option<String>,
}

impl Default for PersistedAudioSettings {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            driver: String::new(),
            device: String::new(),
            block_size: DEFAULT_BLOCK_SIZE,
            low_latency_block_size: DEFAULT_LOW_LATENCY_BLOCK_SIZE,
            channels: Vec::new(),
            master_output: None,
        }
    }
}

const fn default_version() -> u32 {
    CONFIG_VERSION
}

const fn default_block_size() -> u32 {
    DEFAULT_BLOCK_SIZE
}

const fn default_low_latency_block_size() -> u32 {
    DEFAULT_LOW_LATENCY_BLOCK_SIZE
}

impl From<&AudioSettings> for PersistedAudioSettings {
    fn from(settings: &AudioSettings) -> Self {
        Self {
            version: CONFIG_VERSION,
            driver: settings.driver.clone(),
            device: settings.device.clone(),
            block_size: settings.block_size,
            low_latency_block_size: settings.low_latency_block_size,
            channels: settings.channels.clone(),
            master_output: settings.master_output.clone(),
        }
    }
}

impl From<PersistedAudioSettings> for AudioSettings {
    fn from(settings: PersistedAudioSettings) -> Self {
        let block_size = if settings.block_size == 0 {
            DEFAULT_BLOCK_SIZE
        } else {
            settings.block_size
        };
        let low_latency_block_size = settings
            .low_latency_block_size
            .clamp(1, block_size)
            .max(DEFAULT_LOW_LATENCY_BLOCK_SIZE.min(block_size));
        let driver = if settings.driver.is_empty() {
            crate::audio::default_driver_name()
        } else {
            settings.driver
        };
        let device = if settings.device.is_empty() {
            crate::audio::SYSTEM_OUT_DEVICE_ID.to_string()
        } else {
            settings.device
        };
        Self {
            driver,
            device,
            sample_rate: None,
            block_size,
            low_latency_block_size,
            channels: settings.channels,
            master_output: settings.master_output,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load() -> AudioSettings {
    let Some(path) = config_path() else {
        return default_settings();
    };
    load_from_path(&path)
}

#[cfg(target_arch = "wasm32")]
pub fn load() -> AudioSettings {
    let Some(storage) = web_storage() else {
        return default_settings();
    };
    let Ok(Some(text)) = storage.get_item(WEB_KEY) else {
        return default_settings();
    };
    serde_json::from_str::<PersistedAudioSettings>(&text)
        .map_or_else(|_| default_settings(), PersistedAudioSettings::into)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save(settings: &AudioSettings) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    save_to_path(&path, settings)
}

#[cfg(target_arch = "wasm32")]
pub fn save(settings: &AudioSettings) -> Result<(), String> {
    let Some(storage) = web_storage() else {
        return Ok(());
    };
    let text = serde_json::to_string(&PersistedAudioSettings::from(settings))
        .map_err(|err| format!("encode audio settings: {err}"))?;
    storage
        .set_item(WEB_KEY, &text)
        .map_err(|_| "could not save audio settings".to_string())
}

pub fn validated(
    mut settings: AudioSettings,
    options: &[notwig_ui::AudioDriverView],
) -> AudioSettings {
    settings.sample_rate = None;
    let default_driver = crate::audio::default_driver_name();
    let default_driver_available = options.iter().any(|driver| driver.id == default_driver);
    let system_out = settings.device == crate::audio::SYSTEM_OUT_DEVICE_ID;
    if settings.driver.is_empty() && default_driver_available {
        settings.driver.clone_from(&default_driver);
    }
    if !settings.driver.is_empty() {
        if let Some(driver) = options.iter().find(|driver| driver.id == settings.driver) {
            if !settings.device.is_empty()
                && !system_out
                && !driver
                    .devices
                    .iter()
                    .any(|device| device.id == settings.device)
            {
                settings.device.clear();
            }
        } else if system_out {
            // System Out still resolves through the default host when device
            // enumeration is temporarily unavailable.
            settings.driver = if default_driver_available {
                default_driver
            } else {
                String::new()
            };
        } else {
            settings.driver = if default_driver_available {
                default_driver
            } else {
                String::new()
            };
            settings.device.clear();
        }
    } else if !settings.device.is_empty()
        && !system_out
        && !options.iter().any(|driver| {
            driver
                .devices
                .iter()
                .any(|device| device.id == settings.device)
        })
    {
        settings.device.clear();
    }
    if settings.device.is_empty() {
        settings.device = crate::audio::SYSTEM_OUT_DEVICE_ID.to_string();
    }
    let device = options
        .iter()
        .find(|driver| driver.id == settings.driver)
        .and_then(|driver| {
            driver
                .devices
                .iter()
                .find(|device| device.id == settings.device)
        });
    for channel in &mut settings.channels {
        channel.driver.clone_from(&settings.driver);
        channel.device.clone_from(&settings.device);
        let Some(device) = device else {
            continue;
        };
        let available = match channel.direction {
            notwig_ui::AudioChannelDirection::Input => device.input_channels,
            notwig_ui::AudioChannelDirection::Output => device.output_channels,
        };
        if available == 0 {
            continue;
        }
        channel.left_channel = channel.left_channel.min(available - 1);
        match channel.format {
            notwig_ui::AudioChannelFormat::Mono => channel.right_channel = None,
            notwig_ui::AudioChannelFormat::Stereo if available < 2 => {
                channel.format = notwig_ui::AudioChannelFormat::Mono;
                channel.right_channel = None;
            }
            notwig_ui::AudioChannelFormat::Stereo => {
                let mut right = channel
                    .right_channel
                    .unwrap_or_else(|| channel.left_channel.saturating_add(1))
                    .min(available - 1);
                if right == channel.left_channel {
                    right = (channel.left_channel + 1) % available;
                }
                channel.right_channel = Some(right);
            }
        }
    }
    if settings.block_size == 0 {
        settings.block_size = DEFAULT_BLOCK_SIZE;
    }
    settings.low_latency_block_size = settings
        .low_latency_block_size
        .clamp(1, settings.block_size)
        .max(DEFAULT_LOW_LATENCY_BLOCK_SIZE.min(settings.block_size));
    if settings.master_output.as_ref().is_some_and(|id| {
        !settings.channels.iter().any(|channel| {
            channel.direction == notwig_ui::AudioChannelDirection::Output && channel.id == *id
        })
    }) {
        settings.master_output = None;
    }
    settings
}
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
#[must_use]
pub fn load_from_path(path: &Path) -> AudioSettings {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "could not read audio settings"
                );
            }
            return default_settings();
        }
    };
    match serde_json::from_str::<PersistedAudioSettings>(&text) {
        Ok(settings) => settings.into(),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "ignoring invalid audio settings"
            );
            default_settings()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub fn save_to_path(path: &Path, settings: &AudioSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create audio config dir: {err}"))?;
    }
    let text = serde_json::to_string_pretty(&PersistedAudioSettings::from(settings))
        .map_err(|err| format!("encode audio settings: {err}"))?;
    std::fs::write(path, text).map_err(|err| format!("save audio settings: {err}"))
}

fn default_settings() -> AudioSettings {
    PersistedAudioSettings::default().into()
}

#[cfg(not(target_arch = "wasm32"))]
fn config_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/notwig/audio.json");
    #[cfg(not(target_os = "macos"))]
    let path = home.join(".config/notwig/audio.json");
    Some(path)
}

#[cfg(target_arch = "wasm32")]
const WEB_KEY: &str = "notwig.audio";

#[cfg(target_arch = "wasm32")]
fn web_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
