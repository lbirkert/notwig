//! Audio/MIDI processing path for a loaded CLAP plugin.

use crate::FORMAT;
use crate::events::OutputEventsSink;
use crate::plugin::ClapPlugin;
use crate::transport::build_clap_transport;

use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::EventList;
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};
use notwig_rack_core::wrapper::run_audio_block_with;

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::process::{
    CLAP_PROCESS_CONTINUE, CLAP_PROCESS_CONTINUE_IF_NOT_QUIET, CLAP_PROCESS_ERROR,
    CLAP_PROCESS_SLEEP, CLAP_PROCESS_TAIL, clap_process,
};

use std::ptr;

impl Plugin<f32> for ClapPlugin {
    fn process(
        &mut self,
        buffer: &mut AudioBuffer<'_, f32>,
        events: &EventList,
        context: &mut ProcessContext<'_>,
    ) -> Result<ProcessStatus> {
        if !self.is_active() {
            return Err(Error::NotActivated);
        }
        if !self.started_processing {
            if let Some(start) = unsafe { (*self.plugin).start_processing } {
                let ok = unsafe { start(self.plugin) };
                if !ok {
                    return Err(Error::Other(
                        "clap_plugin::start_processing returned false".into(),
                    ));
                }
            }
            self.started_processing = true;
        }

        // Translate the notwig-rack event list into CLAP's typed event
        // unions. Plus any pending parameter changes from
        // set_parameter. The resulting Vec backs the
        // ConvertedInputEvents callbacks for the duration of the
        // process call.
        // Translate the host transport snapshot up front so its
        // backing struct lives for the whole process call.
        let transport_event = context
            .transport
            .map(|t| build_clap_transport(&t, context.sample_rate));

        self.input_events.clear();
        self.input_events.extend_rack_events(events);
        for (param_id, value) in self.pending_param_changes.drain(..) {
            self.input_events.push_param_value(0, param_id, value);
        }
        let input_events = self.input_events.as_clap();
        let mut sink = OutputEventsSink::new(Some(context.output_events));
        let output_events = sink.as_clap();

        // Build per-channel pointer arrays for clap_audio_buffer.
        // The slabs are owned by the plugin and reused across blocks so
        // steady-state processing does not allocate.
        let num_frames = buffer.num_frames();
        self.input_ptrs.clear();
        self.input_ptrs.extend(
            buffer
                .main_inputs()
                .iter()
                .map(|chan| chan.as_ptr().cast_mut()),
        );
        let input_channels = self.input_ptrs.len();
        let input_audio = clap_audio_buffer {
            data32: self.input_ptrs.as_mut_ptr(),
            data64: ptr::null_mut(),
            channel_count: u32::try_from(input_channels).unwrap_or(0),
            latency: 0,
            constant_mask: 0,
        };

        self.output_ptrs.clear();
        self.output_ptrs.extend(
            buffer
                .main_outputs()
                .iter_mut()
                .map(|chan| chan.as_mut_ptr()),
        );
        let output_channels = self.output_ptrs.len();
        let mut output_audio = clap_audio_buffer {
            data32: self.output_ptrs.as_mut_ptr(),
            data64: ptr::null_mut(),
            channel_count: u32::try_from(output_channels).unwrap_or(0),
            latency: 0,
            constant_mask: 0,
        };

        let process = clap_process {
            steady_time: self.steady_time,
            frames_count: u32::try_from(num_frames).unwrap_or(u32::MAX),
            transport: transport_event
                .as_ref()
                .map_or(ptr::null(), std::ptr::from_ref),
            audio_inputs: if input_channels == 0 {
                ptr::null()
            } else {
                &raw const input_audio
            },
            audio_outputs: if output_channels == 0 {
                ptr::null_mut()
            } else {
                &raw mut output_audio
            },
            audio_inputs_count: u32::from(input_channels != 0),
            audio_outputs_count: u32::from(output_channels != 0),
            in_events: &raw const input_events,
            out_events: &raw const output_events,
        };

        let plugin = self.plugin;
        let process_ptr = unsafe { (*plugin).process };
        let status = match process_ptr {
            Some(process_fn) => {
                run_audio_block_with::<ClapPlugin, i32>(FORMAT, CLAP_PROCESS_ERROR, || unsafe {
                    process_fn(plugin, &raw const process)
                })
            }
            None => CLAP_PROCESS_ERROR,
        };

        self.steady_time = self
            .steady_time
            .saturating_add(i64::try_from(num_frames).unwrap_or(0));

        let mapped = map_clap_status(status);
        if mapped == ProcessStatus::Error {
            // The plugin may have written a partial/garbage block before
            // failing. Silence the output rather than passing stale data on.
            for channel in buffer.main_outputs() {
                channel.fill(0.0);
            }
        }
        Ok(mapped)
    }
}

fn map_clap_status(status: i32) -> ProcessStatus {
    match status {
        CLAP_PROCESS_CONTINUE | CLAP_PROCESS_CONTINUE_IF_NOT_QUIET => ProcessStatus::Continue,
        CLAP_PROCESS_SLEEP => ProcessStatus::Sleep,
        CLAP_PROCESS_TAIL => ProcessStatus::Tail { tail_samples: 0 },
        _ => ProcessStatus::Error,
    }
}
