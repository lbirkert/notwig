use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_queue::ArrayQueue;
use notwig_engine::Engine;

use crate::audio::{AudioSnapshot, DspSample, update_snapshot_at};
use crate::clock::Instant;

const WORKER_IDLE_SLEEP: Duration = Duration::from_micros(250);
const HIGH_RENDER_STEP_FRAMES: usize = 64;

/// Preallocated producer/consumer rings shared by the render worker and the
/// device callback. The callback never touches the engine.
pub(crate) struct RenderPipeline {
    pub(crate) high: Arc<ArrayQueue<f32>>,
    pub(crate) low: Arc<ArrayQueue<f32>>,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) output_latency_samples: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RenderPipeline {
    pub(crate) fn start(
        engine: Engine,
        snapshot: Arc<Mutex<AudioSnapshot>>,
        sample_rate: u32,
        device_frames: usize,
        project_frames: usize,
        high_latency_frames: usize,
    ) -> Self {
        let high_target_frames = device_frames.saturating_add(high_latency_frames);
        let high_step_frames = device_frames
            .max(HIGH_RENDER_STEP_FRAMES)
            .min(project_frames.max(1))
            .max(1);
        let low_target_frames = high_step_frames
            .max(usize::try_from(sample_rate / 500).unwrap_or(96))
            .max(1);
        let high = Arc::new(ArrayQueue::new(project_frames.saturating_mul(6).max(8)));
        let low = Arc::new(ArrayQueue::new(
            low_target_frames
                .saturating_add(device_frames.saturating_mul(2))
                .saturating_mul(2)
                .max(8),
        ));
        let ready = Arc::new(AtomicBool::new(false));
        let output_latency_samples = Arc::new(AtomicU64::new(0));
        let running = Arc::new(AtomicBool::new(true));
        let thread = {
            let high = Arc::clone(&high);
            let low = Arc::clone(&low);
            let ready = Arc::clone(&ready);
            let output_latency_samples = Arc::clone(&output_latency_samples);
            let running = Arc::clone(&running);
            thread::Builder::new()
                .name("notwig-project-render".into())
                .spawn(move || {
                    render_loop(
                        engine,
                        snapshot,
                        high,
                        low,
                        ready,
                        output_latency_samples,
                        running,
                        device_frames.max(1),
                        project_frames.max(1),
                        high_target_frames.max(device_frames),
                        low_target_frames,
                        high_step_frames,
                    );
                })
                .ok()
        };
        Self {
            high,
            low,
            ready,
            output_latency_samples,
            running,
            thread,
        }
    }
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn clear_ring(ring: &ArrayQueue<f32>) {
    while ring.pop().is_some() {}
}

#[allow(clippy::too_many_arguments)]
fn render_loop(
    mut engine: Engine,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    high: Arc<ArrayQueue<f32>>,
    low: Arc<ArrayQueue<f32>>,
    ready: Arc<AtomicBool>,
    output_latency_samples: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    device_frames: usize,
    project_frames: usize,
    high_target_frames: usize,
    low_target_frames: usize,
    high_step_frames: usize,
) {
    let mut high_scratch = vec![0.0_f32; high_step_frames * 2];
    let mut low_scratch = vec![0.0_f32; device_frames * 2];
    let mut events = Vec::with_capacity(4096);
    let mut pending_events = Vec::with_capacity(4096);
    let mut pending_frames = 0usize;
    let mut pending_elapsed = Duration::ZERO;
    let mut project_generation = engine.transport_generation();
    let high_block_seconds = project_frames as f64 / f64::from(engine.config().sample_rate.max(1));

    while running.load(Ordering::Relaxed) {
        if snapshot.try_lock().is_ok_and(|snapshot| snapshot.crashed) {
            ready.store(false, Ordering::Relaxed);
            clear_ring(&high);
            clear_ring(&low);
            break;
        }

        let generation = engine.transport_generation();
        if generation != project_generation {
            project_generation = generation;
            ready.store(false, Ordering::Relaxed);
            clear_ring(&high);
            clear_ring(&low);
            pending_frames = 0;
            pending_elapsed = Duration::ZERO;
            pending_events.clear();
        }

        let mut did_work = false;

        if low.len() / 2 < low_target_frames {
            low_scratch.fill(0.0);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                engine.process_low_latency_monitor(&mut low_scratch, device_frames)
            }));
            match outcome {
                Ok(Ok(())) => {
                    for sample in &low_scratch {
                        let _ = low.push(*sample);
                    }
                    did_work = true;
                }
                Ok(Err(error)) => {
                    if let Ok(mut snapshot) = snapshot.try_lock() {
                        snapshot.error = Some(error.to_string());
                    }
                }
                Err(_) => {
                    ready.store(false, Ordering::Relaxed);
                    clear_ring(&high);
                    clear_ring(&low);
                    if let Ok(mut snapshot) = snapshot.try_lock() {
                        snapshot.crashed = true;
                        snapshot.error =
                            Some("Audio engine crashed while monitoring input.".into());
                    }
                    break;
                }
            }
        }

        if high.len() / 2 < high_target_frames {
            let started = Instant::now();
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                engine.process(&mut high_scratch, high_step_frames)
            }));
            match outcome {
                Ok(Ok(())) => {
                    let generation = engine.transport_generation();
                    if generation != project_generation {
                        project_generation = generation;
                        ready.store(false, Ordering::Relaxed);
                        clear_ring(&high);
                        clear_ring(&low);
                        pending_frames = 0;
                        pending_elapsed = Duration::ZERO;
                        pending_events.clear();
                    }
                    let elapsed = started.elapsed();
                    for sample in &high_scratch {
                        let _ = high.push(*sample);
                    }
                    pending_frames = pending_frames.saturating_add(high_step_frames);
                    pending_elapsed = pending_elapsed.saturating_add(elapsed);
                    events.clear();
                    engine.drain_events(&mut events);
                    pending_events.extend_from_slice(&events);
                    if high.len() / 2 >= high_target_frames {
                        ready.store(true, Ordering::Release);
                    }
                    if pending_frames >= project_frames {
                        let output_latency = output_latency_samples.load(Ordering::Relaxed);
                        let queued_frames = u64::try_from(high.len() / 2).unwrap_or(u64::MAX);
                        let audible_playhead =
                            engine.playhead_samples().saturating_sub(queued_frames);
                        let dsp = DspSample {
                            usage_percent: (pending_elapsed.as_secs_f64() / high_block_seconds
                                * 100.0) as f32,
                            load_ms: pending_elapsed.as_secs_f64() as f32 * 1000.0,
                            jitter_percent: 0.0,
                            frames: project_frames as u32,
                            output_latency_samples: output_latency,
                        };
                        update_snapshot_at(
                            &snapshot,
                            &engine,
                            &pending_events,
                            audible_playhead,
                            dsp,
                        );
                        pending_frames = 0;
                        pending_elapsed = Duration::ZERO;
                        pending_events.clear();
                    }
                    did_work = true;
                }
                Ok(Err(error)) => {
                    ready.store(false, Ordering::Relaxed);
                    clear_ring(&high);
                    pending_frames = 0;
                    pending_elapsed = Duration::ZERO;
                    pending_events.clear();
                    if let Ok(mut snapshot) = snapshot.try_lock() {
                        snapshot.error = Some(error.to_string());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(_) => {
                    ready.store(false, Ordering::Relaxed);
                    clear_ring(&high);
                    clear_ring(&low);
                    if let Ok(mut snapshot) = snapshot.try_lock() {
                        snapshot.crashed = true;
                        snapshot.error =
                            Some("Audio engine crashed while processing a block.".into());
                    }
                    break;
                }
            }
        }

        if !did_work {
            thread::sleep(WORKER_IDLE_SLEEP);
        }
    }
    ready.store(false, Ordering::Relaxed);
}
