use std::path::PathBuf;

use notwig_project::{
    ExportBitDepth, ExportFormat, ExportOptions, decode_audio, write_audio_export,
};
use uuid::Uuid;

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-export-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn tone() -> Vec<f32> {
    (0..480)
        .map(|frame| (frame as f32 * 0.05).sin() * 0.25)
        .collect()
}

#[test]
fn writes_16_bit_wav_and_aiff() {
    for (format, extension) in [(ExportFormat::Wav, "wav"), (ExportFormat::Aiff, "aiff")] {
        let path = temp_path(&format!("tone.{extension}"));
        let samples = tone();
        write_audio_export(
            &path,
            48_000,
            1,
            &samples,
            ExportOptions {
                format,
                bit_depth: ExportBitDepth::Int16,
                dither: false,
                normalize: false,
            },
        )
        .unwrap();
        let decoded = decode_audio(&path).unwrap();
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.samples.len(), samples.len());
        for (actual, expected) in decoded.samples.iter().zip(samples.iter()) {
            assert!(
                (actual - expected).abs() < 0.001,
                "{format:?}: {actual} vs {expected}"
            );
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

#[test]
fn normalize_scales_the_peak_to_full_scale() {
    let path = temp_path("normalized.wav");
    let samples = tone();
    write_audio_export(
        &path,
        48_000,
        1,
        &samples,
        ExportOptions {
            format: ExportFormat::Wav,
            bit_depth: ExportBitDepth::Float32,
            dither: false,
            normalize: true,
        },
    )
    .unwrap();
    let decoded = decode_audio(&path).unwrap();
    let peak = decoded
        .samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!((peak - 0.999).abs() < 0.002, "peak was {peak}");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn dithering_is_deterministic() {
    let first = temp_path("first.wav");
    let second = temp_path("second.wav");
    let options = ExportOptions {
        format: ExportFormat::Wav,
        bit_depth: ExportBitDepth::Int16,
        dither: true,
        normalize: false,
    };
    let samples = tone();
    write_audio_export(&first, 48_000, 1, &samples, options).unwrap();
    write_audio_export(&second, 48_000, 1, &samples, options).unwrap();
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
    std::fs::remove_dir_all(first.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(second.parent().unwrap()).unwrap();
}

#[test]
fn writes_24_bit_and_float_wav() {
    for depth in [ExportBitDepth::Int24, ExportBitDepth::Float32] {
        let path = temp_path("depth.wav");
        let samples = tone();
        write_audio_export(
            &path,
            48_000,
            1,
            &samples,
            ExportOptions {
                format: ExportFormat::Wav,
                bit_depth: depth,
                dither: false,
                normalize: false,
            },
        )
        .unwrap();
        let decoded = decode_audio(&path).unwrap();
        assert_eq!(decoded.samples.len(), samples.len());
        for (actual, expected) in decoded.samples.iter().zip(samples.iter()) {
            assert!(
                (actual - expected).abs() < 0.001,
                "{depth:?}: {actual} vs {expected}"
            );
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

#[test]
fn writes_flac_mp3_and_ogg() {
    for (format, extension) in [
        (ExportFormat::Flac, "flac"),
        (ExportFormat::Mp3, "mp3"),
        (ExportFormat::Ogg, "ogg"),
    ] {
        let path = temp_path(&format!("tone.{extension}"));
        let samples: Vec<f32> = (0..48_000)
            .map(|frame| (frame as f32 * 0.05).sin() * 0.25)
            .collect();
        write_audio_export(
            &path,
            48_000,
            1,
            &samples,
            ExportOptions {
                format,
                bit_depth: ExportBitDepth::Int16,
                dither: false,
                normalize: false,
            },
        )
        .unwrap();
        let decoded = decode_audio(&path).unwrap();
        assert_eq!(decoded.sample_rate, 48_000, "{format:?}");
        assert!(!decoded.samples.is_empty(), "{format:?}");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
