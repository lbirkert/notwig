use std::path::PathBuf;

use notwig_project::{
    ExportBitDepth, ExportFormat, ExportOptions, probe_audio, write_audio_export, write_wav_f32,
};
use uuid::Uuid;

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-probe-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn tone(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|frame| (frame as f32 * 0.05).sin() * 0.25)
        .collect()
}

#[test]
fn probes_wav_aiff_flac_mp3_and_ogg_durations() {
    let frames = 48_000;
    let expected = frames as f64 / 48_000.0;
    for (format, extension) in [
        (ExportFormat::Wav, "wav"),
        (ExportFormat::Aiff, "aiff"),
        (ExportFormat::Flac, "flac"),
        (ExportFormat::Mp3, "mp3"),
        (ExportFormat::Ogg, "ogg"),
    ] {
        let path = temp_path(&format!("tone.{extension}"));
        write_audio_export(
            &path,
            48_000,
            1,
            &tone(frames),
            ExportOptions {
                format,
                bit_depth: ExportBitDepth::Int16,
                dither: false,
                normalize: false,
            },
        )
        .unwrap();
        let probe = probe_audio(&path).unwrap();
        assert_eq!(probe.sample_rate, 48_000, "{format:?}");
        assert!(
            (probe.duration_seconds - expected).abs() < 0.2,
            "{format:?}: {} vs {expected}",
            probe.duration_seconds
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

#[test]
fn probes_plain_wav_without_decoding() {
    let path = temp_path("plain.wav");
    let samples = tone(24_000);
    write_wav_f32(&path, 48_000, 1, &samples).unwrap();
    let probe = probe_audio(&path).unwrap();
    assert_eq!(probe.sample_rate, 48_000);
    assert_eq!(probe.channels, 1);
    assert!((probe.duration_seconds - 0.5).abs() < 1e-6);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
