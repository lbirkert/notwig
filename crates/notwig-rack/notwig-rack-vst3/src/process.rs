//! Real-time audio block processing.

use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::EventList;
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};
use notwig_rack_core::wrapper::run_audio_block_with;

use std::ptr;

use vst3::ComWrapper;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, IEventList, IParameterChanges, ProcessData,
    ProcessModes_, SymbolicSampleSizes_,
};
use vst3::Steinberg::kResultOk;

use crate::FORMAT;
use crate::events::{EventBuffer, EventList3};
use crate::params::build_vst3_context;
use crate::plugin::Vst3Plugin;

impl Plugin<f32> for Vst3Plugin {
    fn process(
        &mut self,
        buffer: &mut AudioBuffer<'_, f32>,
        events: &EventList,
        context: &mut ProcessContext<'_>,
    ) -> Result<ProcessStatus> {
        if !self.is_active() {
            return Err(Error::NotActivated);
        }
        let frames = buffer.num_frames();

        // Build an IEventList for the plugin's inputEvents slot.
        // We translate notwig-rack MIDI events to VST3 Event structs.
        let mut translated = EventBuffer::default();
        for event in events {
            translated.push_rack(event);
        }
        let input_events_wrapper = ComWrapper::new(EventList3 {
            events: std::cell::RefCell::new(translated.events),
        });
        let input_events_ptr = input_events_wrapper
            .to_com_ptr::<IEventList>()
            .ok_or_else(|| Error::Other("EventList3 missing IEventList IID".into()))?;
        let output_events_wrapper = ComWrapper::new(EventList3::default());
        let output_events_ptr = output_events_wrapper
            .to_com_ptr::<IEventList>()
            .ok_or_else(|| Error::Other("EventList3 missing IEventList IID".into()))?;

        let main_inputs = buffer.main_inputs();
        let mut input_ptrs: Vec<*mut f32> =
            main_inputs.iter().map(|c| c.as_ptr().cast_mut()).collect();
        let mut input_bus = AudioBusBuffers {
            numChannels: i32::try_from(input_ptrs.len()).unwrap_or(0),
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: input_ptrs.as_mut_ptr(),
            },
        };

        let main_outputs = buffer.main_outputs();
        let mut output_ptrs: Vec<*mut f32> =
            main_outputs.iter_mut().map(|c| c.as_mut_ptr()).collect();
        let mut output_bus = AudioBusBuffers {
            numChannels: i32::try_from(output_ptrs.len()).unwrap_or(0),
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: output_ptrs.as_mut_ptr(),
            },
        };

        // Build the transport context up front so its backing
        // struct outlives the process call.
        let mut process_context = context
            .transport
            .map(|t| build_vst3_context(&t, context.sample_rate));

        let mut data = ProcessData {
            #[allow(clippy::cast_possible_wrap)]
            processMode: ProcessModes_::kRealtime as i32,
            #[allow(clippy::cast_possible_wrap)]
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: i32::try_from(frames).unwrap_or(i32::MAX),
            numInputs: 1,
            numOutputs: 1,
            inputs: &raw mut input_bus,
            outputs: &raw mut output_bus,
            inputParameterChanges: ptr::null_mut::<IParameterChanges>(),
            outputParameterChanges: ptr::null_mut::<IParameterChanges>(),
            inputEvents: input_events_ptr.as_ptr(),
            outputEvents: output_events_ptr.as_ptr(),
            processContext: process_context
                .as_mut()
                .map_or(ptr::null_mut(), std::ptr::from_mut),
        };

        let processor_ptr = self.processor.as_ptr();
        let status = run_audio_block_with::<Vst3Plugin, i32>(FORMAT, -1, || unsafe {
            ((*(*processor_ptr).vtbl).process)(processor_ptr, &raw mut data)
        });
        if status == kResultOk {
            Ok(ProcessStatus::Continue)
        } else {
            // A failed process call may leave the output bus partially written.
            // Clear it so the host never mixes stale/garbage samples.
            for channel in buffer.main_outputs() {
                channel.fill(0.0);
            }
            Ok(ProcessStatus::Error)
        }
    }
}
