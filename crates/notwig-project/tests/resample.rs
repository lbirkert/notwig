use notwig_core::AudioAssetBuffer;
use notwig_project::{ResampleQuality, resample};

fn buffer(sample_rate: u32, channels: u16, samples: Vec<f32>) -> AudioAssetBuffer {
    AudioAssetBuffer {
        sample_rate,
        channels,
        samples,
    }
}

#[test]
fn same_rate_returns_input_unchanged() {
    let input = buffer(48_000, 2, vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6]);
    let output = resample(&input, 48_000, ResampleQuality::High).unwrap();
    assert_eq!(output, input);
}

#[test]
fn upsample_44k1_to_48k_matches_expected_length_and_channels() {
    let in_frames = 4_410;
    let channels = 2_u16;
    let samples = (0..in_frames * usize::from(channels))
        .map(|index| (index as f32 * 0.13).sin() * 0.2)
        .collect();
    let input = buffer(44_100, channels, samples);

    let output = resample(&input, 48_000, ResampleQuality::High).unwrap();

    assert_eq!(output.sample_rate, 48_000);
    assert_eq!(output.channels, channels);
    let expected_frames = (in_frames as f64 * 48_000.0 / 44_100.0).round() as u64;
    let actual_frames = output.frame_count();
    assert!(
        actual_frames.abs_diff(expected_frames) <= 1,
        "expected about {expected_frames} frames, got {actual_frames}"
    );
    assert_eq!(
        output.samples.len(),
        output.frame_count() as usize * usize::from(channels)
    );
}

#[test]
fn preserves_low_frequency_signal_away_from_edges() {
    let in_frames = 4_410;
    let input = buffer(44_100, 1, vec![0.5; in_frames]);

    let output = resample(&input, 48_000, ResampleQuality::High).unwrap();
    let frames = output.frame_count() as usize;
    let mid_start = frames / 2 - 50;
    let mid_end = frames / 2 + 50;
    for &sample in &output.samples[mid_start..mid_end] {
        assert!(
            (sample - 0.5).abs() < 0.01,
            "DC signal not preserved in the middle: {sample}"
        );
    }
}

#[test]
fn known_impulse_does_not_start_with_a_long_silent_prefix() {
    for in_frames in [200, 2_048] {
        let mut samples = vec![0.0_f32; in_frames];
        samples[0] = 1.0;
        let input = buffer(44_100, 1, samples);

        let output = resample(&input, 48_000, ResampleQuality::High).unwrap();

        let prefix_peak = output
            .samples
            .iter()
            .take(16)
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            prefix_peak > 0.1,
            "{in_frames}-frame input starts with silence, prefix peak was {prefix_peak}"
        );

        let leading_silence = output
            .samples
            .iter()
            .take_while(|sample| sample.abs() < 1e-4)
            .count();
        assert!(
            leading_silence < 16,
            "{in_frames}-frame input has a silent prefix of {leading_silence} samples"
        );
    }
}

#[test]
fn short_clip_preserves_signal_and_expected_length() {
    let in_frames = 200;
    let input = buffer(44_100, 1, vec![0.5; in_frames]);

    let output = resample(&input, 48_000, ResampleQuality::High).unwrap();

    let expected_frames = (in_frames as f64 * 48_000.0 / 44_100.0).round() as usize;
    assert_eq!(output.samples.len(), expected_frames);
    for &sample in &output.samples[50..150] {
        assert!(
            (sample - 0.5).abs() < 0.01,
            "short DC clip not preserved: {sample}"
        );
    }
}

#[test]
fn empty_input_stays_empty_at_target_rate() {
    let input = buffer(44_100, 2, Vec::new());
    let output = resample(&input, 48_000, ResampleQuality::Standard).unwrap();
    assert_eq!(output.sample_rate, 48_000);
    assert_eq!(output.channels, 2);
    assert_eq!(output.samples.len(), 0);
}

#[test]
fn zero_target_rate_is_rejected() {
    let input = buffer(44_100, 1, vec![0.1, 0.2]);
    assert!(resample(&input, 0, ResampleQuality::Standard).is_err());
}
