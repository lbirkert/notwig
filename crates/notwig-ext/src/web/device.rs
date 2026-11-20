use std::sync::Arc;

use notwig_core::{ParamId, ParamValue};
use notwig_dsp::{
    CompiledPlan, DeviceFactory, DeviceProcessor, GraphError, Latency, NoteEvent, PlanDevice,
    ProcessContext, ProcessStatus, RenderConfig,
};
use notwig_ext_api::{NOTE_EVENT_BYTES, NoteEvent as ExtensionNoteEvent, encode_note_event};

use crate::web::compiled::WebLoadedExtensions;
use crate::web::instance::WebExtensionInstance;

pub(crate) struct WebExtensionDeviceFactory {
    loaded: Arc<WebLoadedExtensions>,
}

impl WebExtensionDeviceFactory {
    pub(crate) fn new(loaded: Arc<WebLoadedExtensions>) -> Self {
        Self { loaded }
    }
}

impl DeviceFactory for WebExtensionDeviceFactory {
    fn create(
        &self,
        _plan: &CompiledPlan,
        device: &PlanDevice,
    ) -> Option<Box<dyn DeviceProcessor>> {
        let extension = self.loaded.find_type(&device.type_id)?;
        let mut instance =
            WebExtensionInstance::instantiate(&extension.module, &extension.manifest.id)
                .map_err(|error| {
                    tracing::warn!(
                        extension = %extension.manifest.id,
                        type_id = %device.type_id,
                        error = %error,
                        "could not instantiate extension device"
                    );
                    error
                })
                .ok()?;
        let handle = instance.create_device(&device.type_id).ok()?;
        for (param_id, value) in &device.param_values {
            if let Err(error) = instance.set_param(handle, *param_id, value) {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    param = %param_id,
                    error = %error,
                    "could not set extension device parameter"
                );
            }
        }
        Some(Box::new(WebWasmDeviceProcessor {
            instance,
            handle,
            audio_ptr: 0,
            events_ptr: 0,
            scratch: Vec::new(),
            event_scratch: Vec::new(),
            channels: 2,
            latency_samples: device.spec.latency_samples,
            reported: false,
        }))
    }
}

struct WebWasmDeviceProcessor {
    instance: WebExtensionInstance,
    handle: i32,
    audio_ptr: i32,
    events_ptr: i32,
    scratch: Vec<u8>,
    event_scratch: Vec<u8>,
    channels: usize,
    latency_samples: u32,
    reported: bool,
}

impl WebWasmDeviceProcessor {
    fn report(&mut self, error: impl std::fmt::Display) {
        if !self.reported {
            self.reported = true;
            tracing::warn!(error = %error, "extension device processing failed");
        }
    }
}

impl Drop for WebWasmDeviceProcessor {
    fn drop(&mut self) {
        self.instance.destroy_device(self.handle);
    }
}

impl DeviceProcessor for WebWasmDeviceProcessor {
    fn prepare(&mut self, config: RenderConfig) -> Result<(), GraphError> {
        self.channels = usize::from(config.channels.max(1));
        self.instance
            .prepare_device(
                self.handle,
                config.sample_rate,
                config.max_block_size,
                config.channels,
            )
            .map_err(|error| GraphError::DeviceError {
                reason: error.to_string(),
            })?;
        let bytes = config.max_block_size.max(1) * self.channels * std::mem::size_of::<f32>();
        self.audio_ptr =
            self.instance
                .alloc_audio(bytes)
                .map_err(|error| GraphError::DeviceError {
                    reason: error.to_string(),
                })?;
        self.scratch.resize(bytes, 0);
        let event_bytes = config.max_block_size.max(1) * NOTE_EVENT_BYTES;
        self.events_ptr =
            self.instance
                .alloc_audio(event_bytes)
                .map_err(|error| GraphError::DeviceError {
                    reason: error.to_string(),
                })?;
        self.event_scratch.resize(event_bytes, 0);
        self.latency_samples = self.latency_samples.max(self.instance.latency(self.handle));
        Ok(())
    }

    fn reset(&mut self) {
        self.reported = false;
    }

    fn set_param(&mut self, param_id: ParamId, value: &ParamValue) {
        if let Err(error) = self.instance.set_param(self.handle, param_id, value) {
            self.report(error);
        }
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        audio: &mut [f32],
        events: &[NoteEvent],
    ) -> ProcessStatus {
        if audio.is_empty() {
            return ProcessStatus::default();
        }
        let bytes = std::mem::size_of_val(audio);
        if bytes > self.scratch.len() || audio.len() % self.channels != 0 {
            self.report("extension audio buffer is too small");
            return ProcessStatus {
                overloaded: true,
                produced_silence: false,
            };
        }
        let max_events = self.event_scratch.len() / NOTE_EVENT_BYTES;
        let event_count = events.len().min(max_events);
        for (index, event) in events[..event_count].iter().enumerate() {
            let encoded = encode_note_event(&ExtensionNoteEvent::from(*event));
            let start = index * NOTE_EVENT_BYTES;
            self.event_scratch[start..start + NOTE_EVENT_BYTES].copy_from_slice(&encoded);
        }
        let event_bytes = event_count * NOTE_EVENT_BYTES;
        if event_bytes > 0 {
            if let Err(error) = self
                .instance
                .write_bytes(self.events_ptr, &self.event_scratch[..event_bytes])
            {
                self.report(error);
                return ProcessStatus {
                    overloaded: true,
                    produced_silence: false,
                };
            }
        }
        for (chunk, sample) in self.scratch[..bytes]
            .chunks_exact_mut(std::mem::size_of::<f32>())
            .zip(audio.iter())
        {
            chunk.copy_from_slice(&sample.to_le_bytes());
        }
        if let Err(error) = self
            .instance
            .write_bytes(self.audio_ptr, &self.scratch[..bytes])
        {
            self.report(error);
            return ProcessStatus {
                overloaded: true,
                produced_silence: false,
            };
        }
        let frames = audio.len() / self.channels;
        let events_ptr = if event_count == 0 { 0 } else { self.events_ptr };
        let status = match self.instance.process(
            self.handle,
            self.audio_ptr,
            frames,
            events_ptr,
            event_count,
            ctx.sample_position,
            ctx.playing,
        ) {
            Ok(status) => status,
            Err(error) => {
                self.report(error);
                return ProcessStatus {
                    overloaded: true,
                    produced_silence: false,
                };
            }
        };
        if let Err(error) = self
            .instance
            .read_into(self.audio_ptr, &mut self.scratch[..bytes])
        {
            self.report(error);
            return ProcessStatus {
                overloaded: true,
                produced_silence: false,
            };
        }
        for (sample, chunk) in audio.iter_mut().zip(self.scratch[..bytes].chunks_exact(4)) {
            *sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        ProcessStatus {
            overloaded: status != 0,
            produced_silence: false,
        }
    }

    fn latency(&self) -> Latency {
        Latency {
            samples: self.latency_samples,
        }
    }
}
