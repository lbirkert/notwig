use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use notwig_core::SampleRate;

use crate::audio::{SYSTEM_OUT_DEVICE_ID, resolve_device, resolve_host, resolve_input_device};

#[derive(Debug, Clone)]
pub struct LoopbackOptions {
    pub driver: String,
    pub device: String,
    pub input_device: Option<String>,
    pub sample_rate: u32,
    pub duration_ms: u64,
    pub interval_ms: u64,
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoopbackReport {
    pub sample_rate: u32,
    pub input_device: String,
    pub output_device: String,
    pub click_count: usize,
    pub detected_count: usize,
    pub interval_seconds: f64,
    pub latencies_ms: Vec<f64>,
}

impl LoopbackReport {
    #[must_use]
    pub fn summary(&self) -> String {
        if self.latencies_ms.is_empty() {
            return "No loopback transients detected. Check the microphone level and output routing."
                .into();
        }
        let mean = self.latencies_ms.iter().sum::<f64>() / self.latencies_ms.len() as f64;
        let min = self
            .latencies_ms
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max = self
            .latencies_ms
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let variance = self
            .latencies_ms
            .iter()
            .map(|latency| (latency - mean).powi(2))
            .sum::<f64>()
            / self.latencies_ms.len() as f64;
        let drift = self.drift_ms_per_second();
        format!(
            "sample rate: {} Hz\ninput: {}\noutput: {}\nclicks: {} (detected {})\nlatency: mean {:.2} ms, min {:.2} ms, max {:.2} ms, sigma {:.2} ms\ndrift: {:+.3} ms/s\nsamples: mean {:.1}",
            self.sample_rate,
            self.input_device,
            self.output_device,
            self.click_count,
            self.detected_count,
            mean,
            min,
            max,
            variance.sqrt(),
            drift,
            mean * f64::from(self.sample_rate) / 1000.0
        )
    }

    #[must_use]
    pub fn drift_ms_per_second(&self) -> f64 {
        if self.latencies_ms.len() < 2 || self.interval_seconds <= 0.0 {
            return 0.0;
        }
        let mean_x = (self.latencies_ms.len() - 1) as f64 * self.interval_seconds / 2.0;
        let mean_y = self.latencies_ms.iter().sum::<f64>() / self.latencies_ms.len() as f64;
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for (index, latency) in self.latencies_ms.iter().enumerate() {
            let x = index as f64 * self.interval_seconds - mean_x;
            numerator += x * (latency - mean_y);
            denominator += x * x;
        }
        if denominator == 0.0 {
            0.0
        } else {
            numerator / denominator
        }
    }
}

pub fn run(options: &LoopbackOptions) -> Result<LoopbackReport, String> {
    let host = resolve_host(&options.driver)?;
    let output_device = resolve_device(&host, &options.device)?;
    let output_description = output_device
        .description()
        .map_err(|error| format!("output description: {error}"))?;
    let input_name = options.input_device.clone().unwrap_or_else(|| {
        if options.device.is_empty() || options.device == SYSTEM_OUT_DEVICE_ID {
            SYSTEM_OUT_DEVICE_ID.to_string()
        } else {
            options.device.clone()
        }
    });
    let input_device = resolve_input_device(&host, &input_name)?;
    let input_description = input_device
        .description()
        .map_err(|error| format!("input description: {error}"))?;

    let rate = SampleRate::new(options.sample_rate)
        .map_err(|error| error.to_string())?
        .value();
    let interval_samples = options.interval_ms.max(100).saturating_mul(u64::from(rate)) / 1000;
    let lead_in_samples = u64::from(rate) / 5;
    let click_len = (u64::from(rate) / 200).max(16) as usize; // ~5 ms
    let click: Vec<f32> = (0..click_len)
        .map(|index| {
            let envelope = 1.0 - index as f32 / click_len as f32;
            let phase = std::f32::consts::TAU * 1000.0 * index as f32 / rate as f32;
            0.7 * phase.sin() * envelope
        })
        .collect();
    let click = Arc::new(click);
    let clicks = Arc::new(Mutex::new(Vec::<u64>::new()));
    let input = Arc::new(Mutex::new(Vec::<f32>::new()));
    let input_timeline = Arc::new(Mutex::new(Vec::<(usize, u64)>::new()));
    let frames_emitted = Arc::new(AtomicU64::new(0));
    let running = Arc::new(AtomicBool::new(true));

    let output_config = output_stream_config(&output_device, rate)?;
    let input_config = input_stream_config(&input_device, rate)?;
    let output_stream = match output_config.1 {
        SampleFormat::F32 => build_output::<f32>(
            &output_device,
            output_config.0,
            Arc::clone(&click),
            Arc::clone(&clicks),
            Arc::clone(&frames_emitted),
            Arc::clone(&running),
            interval_samples,
            lead_in_samples,
        ),
        SampleFormat::I16 => build_output::<i16>(
            &output_device,
            output_config.0,
            Arc::clone(&click),
            Arc::clone(&clicks),
            Arc::clone(&frames_emitted),
            Arc::clone(&running),
            interval_samples,
            lead_in_samples,
        ),
        SampleFormat::U16 => build_output::<u16>(
            &output_device,
            output_config.0,
            Arc::clone(&click),
            Arc::clone(&clicks),
            Arc::clone(&frames_emitted),
            Arc::clone(&running),
            interval_samples,
            lead_in_samples,
        ),
        SampleFormat::I32 => build_output::<i32>(
            &output_device,
            output_config.0,
            Arc::clone(&click),
            Arc::clone(&clicks),
            Arc::clone(&frames_emitted),
            Arc::clone(&running),
            interval_samples,
            lead_in_samples,
        ),
        other => Err(format!("unsupported loopback output sample format {other}")),
    }?;
    let input_stream = match input_config.1 {
        SampleFormat::F32 => build_input::<f32>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        SampleFormat::I16 => build_input::<i16>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        SampleFormat::U16 => build_input::<u16>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        SampleFormat::I32 => build_input::<i32>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        SampleFormat::I8 => build_input::<i8>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        SampleFormat::U8 => build_input::<u8>(
            &input_device,
            input_config.0,
            Arc::clone(&input),
            Arc::clone(&input_timeline),
            Arc::clone(&running),
        ),
        other => Err(format!("unsupported loopback input sample format {other}")),
    }?;

    output_stream
        .play()
        .map_err(|error| format!("start output: {error}"))?;
    input_stream
        .play()
        .map_err(|error| format!("start input: {error}"))?;
    thread::sleep(Duration::from_millis(options.duration_ms.max(500)));
    running.store(false, Ordering::Relaxed);
    drop(output_stream);
    drop(input_stream);

    let click_times = clicks
        .lock()
        .map_err(|_| "click timestamp lock poisoned".to_string())?
        .clone();
    let samples = input
        .lock()
        .map_err(|_| "input lock poisoned".to_string())?
        .clone();
    let timeline = input_timeline
        .lock()
        .map_err(|_| "input timeline lock poisoned".to_string())?
        .clone();
    if samples.is_empty() {
        return Err("recording produced no input samples".into());
    }
    let detected = detect_onsets(&samples, rate as usize);
    let latencies_ms = match_click_latencies(&click_times, &timeline, &detected, rate);
    if let Some(path) = &options.output {
        notwig_project::write_wav_f32(path, rate, 1, &samples)
            .map_err(|error| error.to_string())?;
    }

    Ok(LoopbackReport {
        sample_rate: rate,
        input_device: input_description.name().to_string(),
        output_device: output_description.name().to_string(),
        click_count: click_times.len(),
        detected_count: detected.len(),
        interval_seconds: options.interval_ms as f64 / 1000.0,
        latencies_ms,
    })
}

fn output_stream_config(
    device: &cpal::Device,
    rate: u32,
) -> Result<(StreamConfig, SampleFormat), String> {
    let default = device
        .default_output_config()
        .map_err(|error| format!("default output config: {error}"))?;
    if default.sample_rate() == rate {
        return Ok((default.config(), default.sample_format()));
    }
    let mut candidate = None;
    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            if range.min_sample_rate() <= rate && rate <= range.max_sample_rate() {
                let config: StreamConfig = range.with_sample_rate(rate).into();
                candidate = Some((config, range.sample_format()));
            }
        }
    }
    candidate.ok_or_else(|| format!("output device does not support {rate} Hz"))
}

fn input_stream_config(
    device: &cpal::Device,
    rate: u32,
) -> Result<(StreamConfig, SampleFormat), String> {
    let default = device
        .default_input_config()
        .map_err(|error| format!("default input config: {error}"))?;
    if default.sample_rate() == rate {
        return Ok((default.config(), default.sample_format()));
    }
    let mut candidate = None;
    if let Ok(configs) = device.supported_input_configs() {
        for range in configs {
            if range.min_sample_rate() <= rate && rate <= range.max_sample_rate() {
                let config: StreamConfig = range.with_sample_rate(rate).into();
                candidate = Some((config, range.sample_format()));
            }
        }
    }
    candidate.ok_or_else(|| format!("input device does not support {rate} Hz"))
}

fn build_output<T>(
    device: &cpal::Device,
    config: StreamConfig,
    click: Arc<Vec<f32>>,
    clicks: Arc<Mutex<Vec<u64>>>,
    frames_emitted: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    interval_samples: u64,
    lead_in_samples: u64,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let rate = config.sample_rate;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                if !running.load(Ordering::Relaxed) {
                    data.fill(T::from_sample(0.0));
                    return;
                }
                let start = frames_emitted.load(Ordering::Relaxed);
                let playback = info.timestamp().playback.as_nanos();
                let mut recorded_clicks = Vec::new();
                for frame in 0..(data.len() / channels) {
                    let position = start + frame as u64;
                    let (phase, click_start) = if position >= lead_in_samples {
                        ((position - lead_in_samples) % interval_samples, true)
                    } else {
                        (0, false)
                    };
                    let sample = if click_start {
                        click.get(phase as usize).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    if click_start && phase == 0 {
                        let host_nanos =
                            playback + (frame as u128 * 1_000_000_000 / u128::from(rate));
                        recorded_clicks.push(u64::try_from(host_nanos).unwrap_or(u64::MAX));
                    }
                    for channel in 0..channels {
                        data[frame * channels + channel] = T::from_sample(sample);
                    }
                }
                frames_emitted.fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
                if !recorded_clicks.is_empty()
                    && let Ok(mut click_times) = clicks.lock()
                {
                    click_times.extend(recorded_clicks);
                }
            },
            |error| eprintln!("loopback output error: {error}"),
            None,
        )
        .map_err(|error| format!("build loopback output: {error}"))
}

fn build_input<T>(
    device: &cpal::Device,
    config: StreamConfig,
    input: Arc<Mutex<Vec<f32>>>,
    timeline: Arc<Mutex<Vec<(usize, u64)>>>,
    running: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    device
        .build_input_stream(
            config,
            move |data: &[T], info: &cpal::InputCallbackInfo| {
                if !running.load(Ordering::Relaxed) {
                    return;
                }
                let capture =
                    u64::try_from(info.timestamp().capture.as_nanos()).unwrap_or(u64::MAX);
                let first_index = input.lock().map_or(0, |samples| samples.len());
                if let Ok(mut samples) = input.lock() {
                    for frame in data.chunks(channels) {
                        if let Some(sample) = frame.first() {
                            samples.push(f32::from_sample(*sample));
                        }
                    }
                }
                if let Ok(mut timeline) = timeline.lock() {
                    timeline.push((first_index, capture));
                }
            },
            |error| eprintln!("loopback input error: {error}"),
            None,
        )
        .map_err(|error| format!("build loopback input: {error}"))
}

fn detect_onsets(samples: &[f32], rate: usize) -> Vec<usize> {
    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak <= 0.001 {
        return Vec::new();
    }
    let threshold = peak * 0.25;
    let min_gap = (rate / 100).max(1);
    let mut onsets = Vec::new();
    let mut last_onset = 0usize;
    let mut was_above = false;
    for (index, sample) in samples.iter().enumerate() {
        let above = sample.abs() >= threshold;
        if above && !was_above && (onsets.is_empty() || index > last_onset + min_gap) {
            onsets.push(index);
            last_onset = index;
        }
        was_above = above;
    }
    onsets
}

fn match_click_latencies(
    click_times: &[u64],
    timeline: &[(usize, u64)],
    onsets: &[usize],
    rate: u32,
) -> Vec<f64> {
    let mut latencies = Vec::new();
    let mut timeline_index = 0usize;
    for (click_index, click_time) in click_times.iter().enumerate() {
        let Some(input_index) = onsets.get(click_index).copied() else {
            break;
        };
        while timeline_index + 1 < timeline.len() && timeline[timeline_index + 1].0 <= input_index {
            timeline_index += 1;
        }
        let Some((block_start, block_time)) = timeline.get(timeline_index) else {
            break;
        };
        let offset_nanos =
            u128::from(u64::try_from(input_index.saturating_sub(*block_start)).unwrap_or(u64::MAX))
                * 1_000_000_000
                / u128::from(rate.max(1));
        let input_time = u64::try_from(u128::from(*block_time) + offset_nanos).unwrap_or(u64::MAX);
        let latency = i128::from(input_time) - i128::from(*click_time);
        latencies.push(latency as f64 / 1_000_000.0);
    }
    latencies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_short_transients() {
        let mut samples = vec![0.0; 4800];
        for index in [100, 2000, 3900] {
            samples[index] = 0.8;
            samples[index + 1] = 0.4;
        }
        assert_eq!(detect_onsets(&samples, 48_000), vec![100, 2000, 3900]);
    }
}
