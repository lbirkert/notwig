use notwig_core::dsp::{
    DeviceProcessor, GraphError, Latency, NoteEvent, ProcessContext, ProcessStatus, RenderConfig,
};
use notwig_core::{DeviceId, ParamId, ParamValue};

use super::DelayLine;

/// Keeps a device processor loaded while allowing the engine to skip its DSP.
///
/// Runtime bypass is deliberately non-structural: the wrapped processor remains
/// instantiated so plugins keep their internal state and do not need to be
/// reloaded when the user enables them again. While bypassed, the wrapper emits
/// the dry input delayed by the wrapped processor's latency so planned latency
/// compensation stays correct without processing audio through the device.
pub(super) struct BypassProcessor {
    device_id: DeviceId,
    bypassed: bool,
    inner: Box<dyn DeviceProcessor>,
    delay: Option<DelayLine>,
}

impl BypassProcessor {
    pub(super) fn new(
        device_id: DeviceId,
        bypassed: bool,
        inner: Box<dyn DeviceProcessor>,
    ) -> Self {
        let latency = inner.latency().samples;
        Self {
            device_id,
            bypassed,
            inner,
            delay: (latency > 0).then(|| DelayLine::new(latency)),
        }
    }
}

impl DeviceProcessor for BypassProcessor {
    fn prepare(&mut self, config: RenderConfig) -> Result<(), GraphError> {
        self.inner.prepare(config)
    }

    fn reset(&mut self) {
        self.inner.reset();
        if let Some(delay) = &mut self.delay {
            delay.reset();
        }
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        self.inner.set_param(param_id, value);
    }

    fn set_nested_param(
        &mut self,
        device_id: DeviceId,
        param_id: ParamId,
        value: &ParamValue,
    ) -> bool {
        self.inner.set_nested_param(device_id, param_id, value)
    }

    fn set_bypass(&mut self, device_id: DeviceId, bypass: bool) -> bool {
        if device_id == self.device_id {
            let was_bypassed = self.bypassed;
            self.bypassed = bypass;
            if bypass
                && !was_bypassed
                && let Some(delay) = &mut self.delay
            {
                delay.reset();
            }
            return true;
        }
        self.inner.set_bypass(device_id, bypass)
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        if self.bypassed {
            if let Some(delay) = &mut self.delay {
                delay.process_in_place(audio, audio.len() / 2);
            }
            return ProcessStatus::default();
        }
        self.inner.process(ctx, audio, events)
    }

    fn latency(&self) -> Latency {
        self.inner.latency()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GainProcessor {
        gain: f32,
        latency: u32,
    }

    impl DeviceProcessor for GainProcessor {
        fn prepare(&mut self, _config: RenderConfig) -> Result<(), GraphError> {
            Ok(())
        }

        fn reset(&mut self) {}

        fn set_param(&mut self, _param_id: ParamId, _value: &ParamValue) {}

        fn process(
            &mut self,
            _ctx: &ProcessContext<'_>,
            audio: &mut [f32],
            _events: &[NoteEvent],
        ) -> ProcessStatus {
            for sample in audio {
                *sample *= self.gain;
            }
            ProcessStatus::default()
        }

        fn latency(&self) -> Latency {
            Latency {
                samples: self.latency,
            }
        }
    }

    fn context() -> ProcessContext<'static> {
        static TRANSPORT: notwig_core::dsp::TransportInfo = notwig_core::dsp::TransportInfo {
            playhead_mus: notwig_core::TimeMus::zero(),
            loop_start_mus: None,
            loop_end_mus: None,
        };
        ProcessContext {
            config: RenderConfig {
                sample_rate: 48_000,
                max_block_size: 8,
                channels: 2,
                tempo_bpm: 120.0,
            },
            sample_position: 0,
            playing: true,
            tempo_bpm: 120.0,
            transport: &TRANSPORT,
        }
    }

    #[test]
    fn bypassed_processor_passes_delayed_dry_signal() {
        let inner: Box<dyn DeviceProcessor> = Box::new(GainProcessor {
            gain: 0.5,
            latency: 2,
        });
        let mut bypass = BypassProcessor::new(DeviceId::nil(), true, inner);
        bypass.prepare(context().config).unwrap();
        let mut audio = [1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0];
        bypass.process(&context(), &mut audio, &[]);
        assert_eq!(audio, [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    }

    #[test]
    fn enabled_processor_runs_normally() {
        let inner: Box<dyn DeviceProcessor> = Box::new(GainProcessor {
            gain: 0.5,
            latency: 2,
        });
        let mut bypass = BypassProcessor::new(DeviceId::nil(), false, inner);
        bypass.prepare(context().config).unwrap();
        let mut audio = [1.0, 1.0, 2.0, 2.0];
        bypass.process(&context(), &mut audio, &[]);
        assert_eq!(audio, [0.5, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn runtime_toggle_skips_and_restores_processing() {
        let inner: Box<dyn DeviceProcessor> = Box::new(GainProcessor {
            gain: 0.5,
            latency: 0,
        });
        let mut bypass = BypassProcessor::new(DeviceId::nil(), false, inner);
        bypass.prepare(context().config).unwrap();
        assert!(bypass.set_bypass(DeviceId::nil(), true));
        let mut audio = [1.0, 1.0];
        bypass.process(&context(), &mut audio, &[]);
        assert_eq!(audio, [1.0, 1.0]);
        assert!(bypass.set_bypass(DeviceId::nil(), false));
        let mut audio = [1.0, 1.0];
        bypass.process(&context(), &mut audio, &[]);
        assert_eq!(audio, [0.5, 0.5]);
    }
}
