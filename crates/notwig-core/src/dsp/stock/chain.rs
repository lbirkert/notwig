use crate::{Db, DeviceId, ParamId, ParamValue};

use super::super::devices::{DeviceProcessor, DeviceSpec, Latency, ProcessContext, ProcessStatus};
use super::super::graph::GraphError;
use super::super::smoothing::SmoothedValue;
use super::specs::{p_dry_wet, p_input_gain, p_output_gain};

pub struct ChainProcessor {
    wet_gain: SmoothedValue,
    output_gain: SmoothedValue,
    dry_wet: SmoothedValue,
    sample_rate: f64,
    dry: Vec<f32>,
    children: Vec<(DeviceId, Box<dyn DeviceProcessor>)>,
}

impl ChainProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        let sample_rate = 48_000.0;
        Self {
            wet_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            output_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            dry_wet: SmoothedValue::new(1.0, sample_rate, 15.0),
            sample_rate,
            dry: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, device_id: DeviceId, processor: Box<dyn DeviceProcessor>) {
        self.children.push((device_id, processor));
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.wet_gain.set_sample_rate(self.sample_rate, 15.0);
        self.output_gain.set_sample_rate(self.sample_rate, 15.0);
        self.dry_wet.set_sample_rate(self.sample_rate, 15.0);
    }
}

impl DeviceProcessor for ChainProcessor {
    fn prepare(&mut self, config: super::super::devices::RenderConfig) -> Result<(), GraphError> {
        self.set_sample_rate(f64::from(config.sample_rate));
        self.wet_gain.snap();
        self.output_gain.snap();
        self.dry_wet.snap();
        self.dry.resize(config.max_block_size.max(1) * 2, 0.0);
        for (_, child) in &mut self.children {
            child.prepare(config)?;
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.wet_gain.snap();
        self.output_gain.snap();
        self.dry_wet.snap();
        self.dry.fill(0.0);
        for (_, child) in &mut self.children {
            child.reset();
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        let db = value.as_f64().unwrap_or(0.0);
        if param_id == p_input_gain() {
            self.wet_gain.set_target(Db(db as f32).to_gain_f64());
        } else if param_id == p_output_gain() {
            self.output_gain.set_target(Db(db as f32).to_gain_f64());
        } else if param_id == p_dry_wet() {
            self.dry_wet.set_target(db.clamp(0.0, 1.0));
        }
    }

    fn set_nested_param(
        &mut self,
        device_id: DeviceId,
        param_id: ParamId,
        value: &ParamValue,
    ) -> bool {
        for (child_id, child) in &mut self.children {
            if *child_id == device_id {
                child.set_param(param_id, value);
                return true;
            }
            if child.set_nested_param(device_id, param_id, value) {
                return true;
            }
        }
        false
    }

    fn set_bypass(&mut self, device_id: DeviceId, bypass: bool) -> bool {
        for (_, child) in &mut self.children {
            if child.set_bypass(device_id, bypass) {
                return true;
            }
        }
        false
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[super::super::devices::NoteEvent],
    ) -> ProcessStatus {
        // The dry buffer is sized in `prepare` to `max_block_size * 2`, so the
        // normal contract (`audio.len() <= max_block_size * 2`) needs no
        // allocation. Oversized blocks are defensive: split them into
        // prepared-capacity chunks so `process` never resizes. Note events are
        // delivered only with the first chunk; the engine never supplies
        // oversized blocks, so this path exists only to keep the callback safe.
        let capacity = self.dry.len();
        if capacity == 0 {
            for (_, child) in &mut self.children {
                child.process(ctx, audio, events);
            }
            return ProcessStatus::default();
        }

        let mut frames_processed = 0u64;
        for (index, chunk) in audio.chunks_mut(capacity).enumerate() {
            let chunk_ctx = if index == 0 {
                *ctx
            } else {
                ProcessContext {
                    sample_position: ctx.sample_position + frames_processed,
                    ..*ctx
                }
            };
            let chunk_events = if index == 0 { events } else { &[] };
            self.process_block(&chunk_ctx, chunk, chunk_events);
            frames_processed += (chunk.len() / 2) as u64;
        }
        ProcessStatus::default()
    }

    fn latency(&self) -> Latency {
        let samples = self
            .children
            .iter()
            .map(|(_, child)| child.latency().samples)
            .fold(0u32, u32::saturating_add);
        Latency { samples }
    }
}

impl ChainProcessor {
    fn process_block(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[super::super::devices::NoteEvent],
    ) {
        // Children process `audio` in place, so the dry snapshot must happen
        // first. Input gain (`wet_gain`) applies to the wet path after the
        // children, then the dry/wet crossfade, then output gain.
        self.dry[..audio.len()].copy_from_slice(audio);
        for (_, child) in &mut self.children {
            child.process(ctx, audio, events);
        }
        for (frame, dry) in audio.chunks_exact_mut(2).zip(self.dry.chunks_exact(2)) {
            let wet_gain = self.wet_gain.next() as f32;
            let output = self.output_gain.next() as f32;
            let mix = self.dry_wet.next() as f32;
            for channel in 0..2 {
                let dry = dry[channel];
                let wet = frame[channel] * wet_gain;
                frame[channel] = (dry * (1.0 - mix) + wet * mix) * output;
            }
        }
    }
}
