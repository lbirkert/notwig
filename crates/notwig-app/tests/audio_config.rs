use std::path::PathBuf;

use notwig_app::audio::{
    AudioSettings, SYSTEM_OUT_DEVICE_ID, resolve_output_device_name, settings_view,
};
use notwig_app::audio_config::{load_from_path, save_to_path, validated};
use notwig_ui::{
    AudioChannelConfig, AudioChannelDirection, AudioChannelFormat, AudioDeviceView, AudioDriverView,
};
use uuid::Uuid;

fn temp_path() -> PathBuf {
    std::env::temp_dir()
        .join(format!("notwig-audio-config-{}", Uuid::new_v4()))
        .join("audio.json")
}

fn settings(driver: &str, device: &str, block_size: u32) -> AudioSettings {
    AudioSettings {
        driver: driver.into(),
        device: device.into(),
        sample_rate: Some(48_000),
        block_size,
        channels: Vec::new(),
        master_output: None,
    }
}

fn options() -> Vec<AudioDriverView> {
    vec![AudioDriverView {
        id: "CoreAudio".into(),
        label: "CoreAudio".into(),
        devices: vec![AudioDeviceView {
            id: "Speakers".into(),
            label: "Speakers".into(),
            channels: 2,
            sample_rates: vec![44_100, 48_000],
        }],
    }]
}

#[test]
fn audio_config_round_trip_persists_driver_device_and_block_size() {
    let path = temp_path();
    let saved = settings("CoreAudio", "Speakers", 256);
    save_to_path(&path, &saved).unwrap();

    let loaded = load_from_path(&path);
    assert_eq!(loaded.driver, "CoreAudio");
    assert_eq!(loaded.device, "Speakers");
    assert_eq!(loaded.block_size, 256);
    assert_eq!(loaded.sample_rate, None);

    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn missing_or_malformed_config_falls_back_to_defaults() {
    let missing = temp_path();
    let loaded = load_from_path(&missing);
    assert_eq!(loaded.driver, "");
    assert_eq!(loaded.device, "");
    assert_eq!(loaded.block_size, 512);

    let malformed = temp_path();
    std::fs::create_dir_all(malformed.parent().unwrap()).unwrap();
    std::fs::write(&malformed, b"{not json").unwrap();
    let loaded = load_from_path(&malformed);
    assert_eq!(loaded.driver, "");
    assert_eq!(loaded.device, "");
    assert_eq!(loaded.block_size, 512);

    std::fs::remove_dir_all(malformed.parent().unwrap()).unwrap();
}

#[test]
fn config_tolerates_missing_and_extra_fields() {
    let path = temp_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"driver":"CoreAudio","block_size":128,"future_field":true}"#,
    )
    .unwrap();

    let loaded = load_from_path(&path);
    assert_eq!(loaded.driver, "CoreAudio");
    assert_eq!(loaded.device, "");
    assert_eq!(loaded.block_size, 128);

    std::fs::write(&path, br#"{"driver":"CoreAudio","device":"Speakers"}"#).unwrap();
    let loaded = load_from_path(&path);
    assert_eq!(loaded.block_size, 512);

    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn system_out_resolves_to_the_default_output_device() {
    let default = resolve_output_device_name("", "");
    let system_out = resolve_output_device_name("", SYSTEM_OUT_DEVICE_ID);
    match (default, system_out) {
        (Ok(default_name), Ok(system_name)) => assert_eq!(default_name, system_name),
        (Err(default_error), Err(system_error)) => {
            assert_eq!(default_error, system_error);
        }
        (default, system_out) => {
            panic!(
                "empty and system-out should resolve the same way: {default:?} vs {system_out:?}"
            )
        }
    }
}

#[test]
fn settings_view_warns_when_system_out_is_selected() {
    let system_out = settings_view(Vec::new(), &settings("", SYSTEM_OUT_DEVICE_ID, 512), None);
    assert_eq!(
        system_out.warning.as_deref(),
        Some("System Out follows the operating system's default output device.")
    );

    let hardware = settings_view(Vec::new(), &settings("", "Speakers", 512), None);
    assert_eq!(hardware.warning, None);
}

#[test]
fn validation_preserves_system_out_when_the_device_list_is_empty() {
    let empty_driver = vec![AudioDriverView {
        id: "CoreAudio".into(),
        label: "CoreAudio".into(),
        devices: Vec::new(),
    }];
    let kept = validated(
        settings("CoreAudio", SYSTEM_OUT_DEVICE_ID, 256),
        &empty_driver,
    );
    assert_eq!(kept.driver, "CoreAudio");
    assert_eq!(kept.device, SYSTEM_OUT_DEVICE_ID);

    let no_drivers = validated(settings("CoreAudio", SYSTEM_OUT_DEVICE_ID, 256), &[]);
    assert_eq!(no_drivers.driver, "");
    assert_eq!(no_drivers.device, SYSTEM_OUT_DEVICE_ID);

    let no_driver = validated(settings("", SYSTEM_OUT_DEVICE_ID, 256), &[]);
    assert_eq!(no_driver.device, SYSTEM_OUT_DEVICE_ID);
}

#[test]
fn system_out_round_trips_through_persisted_config() {
    let path = temp_path();
    let saved = settings("CoreAudio", SYSTEM_OUT_DEVICE_ID, 128);
    save_to_path(&path, &saved).unwrap();

    let loaded = load_from_path(&path);
    assert_eq!(loaded.driver, "CoreAudio");
    assert_eq!(loaded.device, SYSTEM_OUT_DEVICE_ID);
    assert_eq!(loaded.block_size, 128);

    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn validation_drops_missing_driver_or_device() {
    let options = options();

    let missing_driver = validated(settings("Missing", "Speakers", 256), &options);
    assert_eq!(missing_driver.driver, "");
    assert_eq!(missing_driver.device, "");
    assert_eq!(missing_driver.block_size, 256);

    let missing_device = validated(settings("CoreAudio", "Missing", 256), &options);
    assert_eq!(missing_device.driver, "CoreAudio");
    assert_eq!(missing_device.device, "");

    let available = validated(settings("CoreAudio", "Speakers", 256), &options);
    assert_eq!(available.driver, "CoreAudio");
    assert_eq!(available.device, "Speakers");

    let zero_block = validated(settings("CoreAudio", "Speakers", 0), &options);
    assert_eq!(zero_block.block_size, 512);
}

fn named_channel() -> AudioChannelConfig {
    AudioChannelConfig {
        id: "vocals".into(),
        name: "Vocals".into(),
        direction: AudioChannelDirection::Input,
        format: AudioChannelFormat::Mono,
        driver: "CoreAudio".into(),
        device: "Interface".into(),
        first_channel: 2,
    }
}

#[test]
fn audio_config_round_trip_persists_named_channels() {
    let path = temp_path();
    let mut saved = settings("CoreAudio", "Speakers", 256);
    saved.channels = vec![
        named_channel(),
        AudioChannelConfig {
            id: "main-out".into(),
            name: "Main Out".into(),
            direction: AudioChannelDirection::Output,
            format: AudioChannelFormat::Stereo,
            driver: "CoreAudio".into(),
            device: "Speakers".into(),
            first_channel: 0,
        },
    ];
    saved.master_output = Some("main-out".into());
    save_to_path(&path, &saved).unwrap();

    let loaded = load_from_path(&path);
    assert_eq!(loaded.channels, saved.channels);
    assert_eq!(loaded.master_output.as_deref(), Some("main-out"));

    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn validation_drops_missing_master_output() {
    let mut configured = settings("CoreAudio", "Speakers", 256);
    configured.master_output = Some("missing".into());
    let without_output = validated(configured, &options());
    assert_eq!(without_output.master_output, None);

    let mut mono_output = settings("CoreAudio", "Speakers", 256);
    mono_output.channels = vec![AudioChannelConfig {
        id: "main-out".into(),
        name: "Main Out".into(),
        direction: AudioChannelDirection::Output,
        format: AudioChannelFormat::Mono,
        driver: "CoreAudio".into(),
        device: "Speakers".into(),
        first_channel: 1,
    }];
    mono_output.master_output = Some("main-out".into());
    let with_output = validated(mono_output, &options());
    assert_eq!(with_output.master_output.as_deref(), Some("main-out"));
}
