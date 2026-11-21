use notwig_core::AudioAssetBuffer;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::error::ProjectError;

const CHUNK_SIZE: usize = 1024;
/// Leading silence fed to the resampler so its FFT overlap windows can warm up
/// outside the audible signal. Without this the first output chunk contains a
/// loud startup transient for high-frequency material.
const LEAD_PAD_FRAMES: usize = CHUNK_SIZE * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleQuality {
    Standard,
    High,
}

pub fn resample(
    input: &AudioAssetBuffer,
    target_rate: u32,
    quality: ResampleQuality,
) -> Result<AudioAssetBuffer, ProjectError> {
    if target_rate == 0 {
        return Err(ProjectError::Resample(
            "target rate must be positive".into(),
        ));
    }
    if input.sample_rate == target_rate {
        return Ok(input.clone());
    }

    let channels = usize::from(input.channels.max(1));
    let in_frames = input.samples.len() / channels;
    if input.channels == 0 || in_frames == 0 {
        return Ok(AudioAssetBuffer {
            sample_rate: target_rate,
            channels: input.channels,
            samples: Vec::new(),
        });
    }

    // Fixed-rate FFT resampling is both fast and high quality, so both quality
    // variants share the same implementation.
    let _ = quality;

    // Feed leading silence to the resampler so the first FFT overlap window is
    // not built from an abrupt signal start. `process_all` trims its own delay,
    // but the first chunk still carries a startup transient unless it has this
    // run-in. The corresponding output frames are discarded below.
    let processed_frames = LEAD_PAD_FRAMES.saturating_add(in_frames);
    let mut padded = vec![0.0; processed_frames * channels];
    let lead_samples = LEAD_PAD_FRAMES * channels;
    let used = in_frames * channels;
    padded[lead_samples..lead_samples + used].copy_from_slice(&input.samples[..used]);

    let mut resampler = Fft::<f32>::new(
        input.sample_rate as usize,
        target_rate as usize,
        CHUNK_SIZE,
        channels,
        FixedSync::Input,
    )
    .map_err(|err| ProjectError::Resample(format!("failed to build resampler: {err}")))?;

    let input_adapter = InterleavedSlice::new(&padded, channels, processed_frames)
        .map_err(|err| ProjectError::Resample(format!("invalid input buffer: {err}")))?;

    let output = resampler
        .process_all(&input_adapter, processed_frames, None)
        .map_err(|err| ProjectError::Resample(format!("resampling failed: {err}")))?;

    let output_frames =
        (in_frames as f64 * f64::from(target_rate) / f64::from(input.sample_rate)).round() as usize;
    let lead_output_frames = (LEAD_PAD_FRAMES as f64 * f64::from(target_rate)
        / f64::from(input.sample_rate))
    .floor() as usize;
    let mut output_samples = output.take_data();
    let discard = lead_output_frames
        .saturating_mul(channels)
        .min(output_samples.len());
    output_samples.drain(0..discard);
    let expected_samples = output_frames * channels;
    if output_samples.len() < expected_samples {
        output_samples.resize(expected_samples, 0.0);
    }
    output_samples.truncate(expected_samples);

    Ok(AudioAssetBuffer {
        sample_rate: target_rate,
        channels: input.channels,
        samples: output_samples,
    })
}
