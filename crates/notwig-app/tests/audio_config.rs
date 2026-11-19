use std::path::PathBuf;

use notwig_app::audio::AudioSettings;
use notwig_app::audio_config::{load_from_path, save_to_path, validated};
use notwig_ui::{AudioDeviceView, AudioDriverView};
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
    }
}

fn options() -> Vec<AudioDriverView> {
    vec![AudioDriverView {
        id: "CoreAudio".into(),
        label: "CoreAudio".into(),
        devices: vec![AudioDeviceView {
            id: "Speakers".into(),
            label: "Speakers".into(),
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
