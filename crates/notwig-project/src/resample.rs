use notwig_core::AudioAssetBuffer;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::error::ProjectError;

const CHUNK_SIZE: usize = 1024;

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

    // `process_all` only trims its initial delay while iterating over full chunks.
    // Clips shorter than one chunk are padded with trailing zeros so the delay is
    // trimmed too; the extra frames are discarded below.
    let mut padded = Vec::new();
    let (samples, processed_frames) = if in_frames <= CHUNK_SIZE {
        let padded_frames = CHUNK_SIZE + 1;
        padded.resize(padded_frames * channels, 0.0);
        let used = in_frames * channels;
        padded[..used].copy_from_slice(&input.samples[..used]);
        (padded.as_slice(), padded_frames)
    } else {
        (input.samples.as_slice(), in_frames)
    };

    let mut resampler = Fft::<f32>::new(
        input.sample_rate as usize,
        target_rate as usize,
        CHUNK_SIZE,
        channels,
        FixedSync::Input,
    )
    .map_err(|err| ProjectError::Resample(format!("failed to build resampler: {err}")))?;

    let input_adapter = InterleavedSlice::new(samples, channels, processed_frames)
        .map_err(|err| ProjectError::Resample(format!("invalid input buffer: {err}")))?;

    let output = resampler
        .process_all(&input_adapter, processed_frames, None)
        .map_err(|err| ProjectError::Resample(format!("resampling failed: {err}")))?;

    let output_frames =
        (in_frames as f64 * f64::from(target_rate) / f64::from(input.sample_rate)).round() as usize;
    let mut output_samples = output.take_data();
    output_samples.truncate(output_frames * channels);

    Ok(AudioAssetBuffer {
        sample_rate: target_rate,
        channels: input.channels,
        samples: output_samples,
    })
}
