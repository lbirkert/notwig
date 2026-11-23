use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::DeviceTrait;
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
#[cfg(not(target_arch = "wasm32"))]
use crossbeam_queue::ArrayQueue;
use notwig_engine::Engine;

use super::snapshot::{AudioSnapshot, DspSample, update_snapshot};
use super::with_output_sample_format;
use crate::clock::Instant;

/// Errors that mean the current output stream cannot continue and the app must
/// reopen the configured device. `DeviceChanged` is handled separately because
/// the host may keep the rerouted stream alive.
#[doc(hidden)]
#[must_use]
pub fn is_device_loss_error(kind: cpal::ErrorKind) -> bool {
    matches!(
        kind,
        cpal::ErrorKind::DeviceNotAvailable
            | cpal::ErrorKind::StreamInvalidated
            | cpal::ErrorKind::HostUnavailable
    )
}

#[doc(hidden)]
#[must_use]
pub fn is_device_change_error(kind: cpal::ErrorKind) -> bool {
    kind == cpal::ErrorKind::DeviceChanged
}

fn publish_stream_error(snapshot: &Arc<Mutex<AudioSnapshot>>, err: cpal::Error) {
    if let Ok(mut snapshot) = snapshot.lock() {
        let kind = err.kind();
        if is_device_loss_error(kind) {
            snapshot.running = false;
            snapshot.playing = false;
            snapshot.device_lost = true;
            snapshot.device_lost_reason = Some(err.to_string());
            snapshot.error = None;
            snapshot.meters.clear();
            snapshot.device_meters.clear();
        } else if is_device_change_error(kind) {
            snapshot.device_changed = true;
        } else {
            snapshot.error = Some(err.to_string());
        }
    }
}

fn callback_output_latency_samples(info: &cpal::OutputCallbackInfo, sample_rate: u32) -> u64 {
    let timestamp = info.timestamp();
    (timestamp
        .playback
        .duration_since(timestamp.callback)
        .as_secs_f64()
        * f64::from(sample_rate))
    .round() as u64
}

fn write_output<T, F>(
    data: &mut [T],
    channels: usize,
    output_route: (usize, Option<usize>),
    mut next_frame: F,
) where
    T: SizedSample + FromSample<f32>,
    F: FnMut(usize) -> (f32, f32),
{
    for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
        let (left, right) = next_frame(frame_index);
        for (channel_index, sample) in frame.iter_mut().enumerate() {
            let value = if channel_index == output_route.0 {
                left
            } else if Some(channel_index) == output_route.1 {
                right
            } else {
                0.0
            };
            *sample = T::from_sample(value);
        }
    }
}

pub(super) fn build_inline_for_format(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    engine: Engine,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
    project_frames: usize,
    high_latency_frames: usize,
) -> Result<cpal::Stream, String> {
    with_output_sample_format!(
        sample_format,
        T,
        build::<T>(
            device,
            config,
            engine,
            snapshot,
            output_route,
            project_frames,
            high_latency_frames,
        )
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub(super) fn build_rendered_for_format(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    high: Arc<ArrayQueue<f32>>,
    low: Arc<ArrayQueue<f32>>,
    ready: Arc<AtomicBool>,
    output_latency_samples: Arc<AtomicU64>,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
) -> Result<cpal::Stream, String> {
    with_output_sample_format!(
        sample_format,
        T,
        build_rendered::<T>(
            device,
            config,
            high,
            low,
            ready,
            output_latency_samples,
            snapshot,
            output_route,
        )
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn build_rendered<T>(
    device: &cpal::Device,
    config: StreamConfig,
    high: Arc<ArrayQueue<f32>>,
    low: Arc<ArrayQueue<f32>>,
    ready: Arc<AtomicBool>,
    output_latency_samples: Arc<AtomicU64>,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    let error_snapshot = Arc::clone(&snapshot);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                if frames == 0 {
                    return;
                }
                let latency = callback_output_latency_samples(info, sample_rate);
                output_latency_samples.store(latency, Ordering::Relaxed);
                let high_ready = ready.load(Ordering::Acquire);
                write_output(data, channels, output_route, |_| {
                    let low_left = low.pop().unwrap_or(0.0);
                    let low_right = low.pop().unwrap_or(0.0);
                    let high_left = if high_ready {
                        high.pop().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    let high_right = if high_ready {
                        high.pop().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    (high_left + low_left, high_right + low_right)
                });
            },
            move |err| publish_stream_error(&error_snapshot, err),
            None,
        )
        .map_err(|err| format!("build audio stream: {err}"))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut engine: Engine,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    output_route: (usize, Option<usize>),
    project_frames: usize,
    high_latency_frames: usize,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let sample_rate = config.sample_rate;
    let initial_frames = match config.buffer_size {
        cpal::BufferSize::Fixed(frames) => usize::try_from(frames).unwrap_or(1024),
        cpal::BufferSize::Default => 1024,
    };
    let project_frames = project_frames.max(initial_frames.max(1));
    let mut low_scratch: Vec<f32> = vec![0.0; initial_frames.max(1) * 2];
    let mut project_scratch: Vec<f32> = vec![0.0; project_frames * 2];
    // Worst case is the fill target plus one full project chunk of overshoot.
    // Since device_frames <= project_frames, 6 * project_frames covers it.
    let mut project_ring =
        std::collections::VecDeque::with_capacity(project_frames.saturating_mul(6));
    let mut project_generation = engine.transport_generation();
    let mut events = Vec::with_capacity(4096);
    let mut last_callback: Option<Instant> = None;
    let error_snapshot = Arc::clone(&snapshot);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                if frames == 0 {
                    return;
                }
                if low_scratch.len() < frames * 2 {
                    low_scratch.resize(frames * 2, 0.0);
                }
                let mut processed = false;
                let started = Instant::now();
                let period = last_callback.map(|previous| (started - previous).as_secs_f64());
                last_callback = Some(started);
                let crashed = snapshot.try_lock().is_ok_and(|snapshot| snapshot.crashed);
                if !crashed {
                    let mut crashed_now = false;
                    let target_len = frames.saturating_add(high_latency_frames) * 2;
                    while project_ring.len() < target_len {
                        let outcome = catch_unwind(AssertUnwindSafe(|| {
                            engine
                                .process(&mut project_scratch[..project_frames * 2], project_frames)
                        }));
                        match outcome {
                            Ok(Ok(())) => {
                                let generation = engine.transport_generation();
                                if generation != project_generation {
                                    project_ring.clear();
                                    project_generation = generation;
                                }
                                project_ring
                                    .extend(project_scratch[..project_frames * 2].iter().copied());
                            }
                            Ok(Err(err)) => {
                                project_ring.clear();
                                if let Ok(mut snapshot) = snapshot.try_lock() {
                                    snapshot.error = Some(err.to_string());
                                }
                                break;
                            }
                            Err(_) => {
                                project_ring.clear();
                                crashed_now = true;
                                if let Ok(mut snapshot) = snapshot.try_lock() {
                                    snapshot.crashed = true;
                                    snapshot.error = Some(
                                        "Audio engine crashed while processing a block.".into(),
                                    );
                                }
                                break;
                            }
                        }
                    }
                    if !crashed_now {
                        low_scratch[..frames * 2].fill(0.0);
                        let outcome = catch_unwind(AssertUnwindSafe(|| {
                            engine
                                .process_low_latency_monitor(&mut low_scratch[..frames * 2], frames)
                        }));
                        if outcome.is_err() {
                            low_scratch[..frames * 2].fill(0.0);
                            if let Ok(mut snapshot) = snapshot.try_lock() {
                                snapshot.crashed = true;
                                snapshot.error =
                                    Some("Audio engine crashed while monitoring input.".into());
                            }
                        }
                    }
                    processed = !crashed_now;
                }
                if processed {
                    let block_seconds = frames as f64 / f64::from(sample_rate.max(1));
                    let elapsed = started.elapsed();
                    let output_latency_samples = callback_output_latency_samples(info, sample_rate);
                    let dsp = DspSample {
                        usage_percent: (elapsed.as_secs_f64() / block_seconds * 100.0) as f32,
                        load_ms: elapsed.as_secs_f64() as f32 * 1000.0,
                        jitter_percent: period.map_or(0.0, |period| {
                            ((period - block_seconds).abs() / block_seconds * 100.0) as f32
                        }),
                        frames: frames as u32,
                        output_latency_samples,
                    };
                    events.clear();
                    engine.drain_events(&mut events);
                    update_snapshot(&snapshot, &engine, &events, dsp);
                }
                write_output(data, channels, output_route, |frame_index| {
                    if processed {
                        (
                            project_ring.pop_front().unwrap_or(0.0)
                                + low_scratch.get(frame_index * 2).copied().unwrap_or(0.0),
                            project_ring.pop_front().unwrap_or(0.0)
                                + low_scratch.get(frame_index * 2 + 1).copied().unwrap_or(0.0),
                        )
                    } else {
                        (0.0, 0.0)
                    }
                });
            },
            move |err| publish_stream_error(&error_snapshot, err),
            None,
        )
        .map_err(|err| format!("build audio stream: {err}"))
}
