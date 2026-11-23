use super::{Engine, EngineError};

impl Engine {
    pub fn begin_offline_render(&mut self, start_sample: u64) {
        self.transport.playing = true;
        self.transport.playhead_samples = start_sample as f64;
        self.transport.loop_enabled = false;
        self.all_notes_off = true;
        self.offline = true;
        self.events.clear();
        if let Some(runtime) = &mut self.runtime {
            runtime.reset();
            runtime.set_measure_meters(false);
            runtime.prime_notes_at(start_sample);
        }
    }

    pub fn render_offline_block(&mut self, frames: usize) -> Result<Vec<f32>, EngineError> {
        let frames = frames.clamp(1, self.config.max_block_size.max(1));
        let mut output = vec![0.0; frames * 2];
        self.process(&mut output, frames)?;
        Ok(output)
    }

    pub fn render_offline(
        &mut self,
        start_sample: u64,
        end_sample: u64,
    ) -> Result<Vec<f32>, EngineError> {
        if end_sample <= start_sample {
            return Ok(Vec::new());
        }
        self.begin_offline_render(start_sample);
        let total = usize::try_from(end_sample - start_sample).unwrap_or(0);
        let mut output = vec![0.0; total * 2];
        let block = self.config.max_block_size.max(1);
        let mut offset = 0usize;
        while offset < total {
            let frames = block.min(total - offset);
            self.process(&mut output[offset * 2..(offset + frames) * 2], frames)?;
            offset += frames;
        }
        Ok(output)
    }
}
