use crate::{Db, ParamId, ParamValue};

use super::super::devices::{DeviceProcessor, DeviceSpec, ProcessContext, ProcessStatus};
use super::super::graph::GraphError;
use super::super::smoothing::SmoothedValue;
use super::specs::{
    p_dry_wet, p_input_gain, p_output_gain, p_pan, p_polarity_l, p_polarity_r, p_swap_lr, p_width,
};

#[derive(Debug, Clone)]
struct ToolSmoothing {
    input_gain: SmoothedValue,
    output_gain: SmoothedValue,
    pan: SmoothedValue,
    width: SmoothedValue,
    dry_wet: SmoothedValue,
    polarity_l: SmoothedValue,
    polarity_r: SmoothedValue,
    swap_lr: SmoothedValue,
    sample_rate: f64,
}

impl ToolSmoothing {
    fn new(sample_rate: f64) -> Self {
        Self {
            input_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            output_gain: SmoothedValue::new(1.0, sample_rate, 15.0),
            pan: SmoothedValue::new(0.0, sample_rate, 15.0),
            width: SmoothedValue::new(1.0, sample_rate, 15.0),
            dry_wet: SmoothedValue::new(1.0, sample_rate, 15.0),
            polarity_l: SmoothedValue::new(1.0, sample_rate, 8.0),
            polarity_r: SmoothedValue::new(1.0, sample_rate, 8.0),
            swap_lr: SmoothedValue::new(0.0, sample_rate, 8.0),
            sample_rate,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.input_gain.set_sample_rate(self.sample_rate, 15.0);
        self.output_gain.set_sample_rate(self.sample_rate, 15.0);
        self.pan.set_sample_rate(self.sample_rate, 15.0);
        self.width.set_sample_rate(self.sample_rate, 15.0);
        self.dry_wet.set_sample_rate(self.sample_rate, 15.0);
        self.polarity_l.set_sample_rate(self.sample_rate, 8.0);
        self.polarity_r.set_sample_rate(self.sample_rate, 8.0);
        self.swap_lr.set_sample_rate(self.sample_rate, 8.0);
    }

    fn snap(&mut self) {
        self.input_gain.snap();
        self.output_gain.snap();
        self.pan.snap();
        self.width.snap();
        self.dry_wet.snap();
        self.polarity_l.snap();
        self.polarity_r.snap();
        self.swap_lr.snap();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        let value = value.as_f64().unwrap_or(0.0);
        if param_id == p_input_gain() {
            self.input_gain.set_target(Db(value as f32).to_gain_f64());
        } else if param_id == p_output_gain() {
            self.output_gain.set_target(Db(value as f32).to_gain_f64());
        } else if param_id == p_pan() {
            self.pan.set_target(value.clamp(-1.0, 1.0));
        } else if param_id == p_width() {
            self.width.set_target(value.clamp(0.0, 2.0));
        } else if param_id == p_polarity_l() {
            self.polarity_l
                .set_target(if value > 0.5 { -1.0 } else { 1.0 });
        } else if param_id == p_swap_lr() {
            self.swap_lr.set_target(if value > 0.5 { 1.0 } else { 0.0 });
        } else if param_id == p_polarity_r() {
            self.polarity_r
                .set_target(if value > 0.5 { -1.0 } else { 1.0 });
        } else if param_id == p_dry_wet() {
            self.dry_wet.set_target(value.clamp(0.0, 1.0));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolProcessor {
    smooth: ToolSmoothing,
}

impl ToolProcessor {
    #[must_use]
    pub fn new(_spec: &DeviceSpec) -> Self {
        Self {
            smooth: ToolSmoothing::new(48_000.0),
        }
    }
}

impl DeviceProcessor for ToolProcessor {
    fn prepare(&mut self, config: super::super::devices::RenderConfig) -> Result<(), GraphError> {
        self.smooth.set_sample_rate(f64::from(config.sample_rate));
        self.smooth.snap();
        Ok(())
    }

    fn reset(&mut self) {
        self.smooth.snap();
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        self.smooth.set_param(param_id, value);
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        _events: &[super::super::devices::NoteEvent],
    ) -> ProcessStatus {
        for frame in audio.chunks_exact_mut(2) {
            let input_gain = self.smooth.input_gain.next() as f32;
            let output_gain = self.smooth.output_gain.next() as f32;
            let pan = self.smooth.pan.next() as f32;
            let width = self.smooth.width.next() as f32;
            let wet = self.smooth.dry_wet.next() as f32;
            let polarity_l = self.smooth.polarity_l.next() as f32;
            let polarity_r = self.smooth.polarity_r.next() as f32;
            let swap = self.smooth.swap_lr.next() as f32;
            let (pan_l, pan_r) = crate::Pan(pan).gains();
            let dry_l = frame[0];
            let dry_r = frame[1];
            let mut l = dry_l * input_gain * polarity_l;
            let mut r = dry_r * input_gain * polarity_r;
            if swap > 0.0 {
                let swapped_l = l * (1.0 - swap) + r * swap;
                let swapped_r = r * (1.0 - swap) + l * swap;
                l = swapped_l;
                r = swapped_r;
            }
            let mid = f32::midpoint(l, r);
            let side = (l - r) * 0.5 * width;
            l = (mid + side) * pan_l;
            r = (mid - side) * pan_r;
            frame[0] = (dry_l * (1.0 - wet) + l * wet) * output_gain;
            frame[1] = (dry_r * (1.0 - wet) + r * wet) * output_gain;
        }
        ProcessStatus::default()
    }
}
