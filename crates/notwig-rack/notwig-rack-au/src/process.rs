use notwig_rack_core::buffer::AudioBuffer;
use notwig_rack_core::error::{Error, Result};
use notwig_rack_core::events::EventList;
use notwig_rack_core::plugin::{Plugin, PluginCore, ProcessContext, ProcessStatus};

use objc2_audio_toolbox::{
    AudioComponentInstance, AudioUnitRender, AudioUnitRenderActionFlags, MusicDeviceMIDIEvent,
};
use objc2_core_audio_types::{
    AudioBuffer as CAAudioBuffer, AudioBufferList, AudioTimeStamp, AudioTimeStampFlags,
};

use std::ptr;

use crate::plugin::AuPlugin;
use crate::transport::update_host_transport;

/// Route one notwig-rack [`notwig_rack_core::events::Event`] into the AU via
/// `MusicDeviceMIDIEvent`. Non-MIDI events (param automation,
/// transport flags) are skipped — AU parameter automation goes
/// through `AudioUnitSetParameter` with a sample offset, and
/// transport flips are surfaced via the render-callback context.
fn send_midi(unit: AudioComponentInstance, event: &notwig_rack_core::events::Event) {
    use notwig_rack_core::events::{EventBody, MidiData};
    let offset = event.sample_offset;
    let (status, d1, d2) = match event.body {
        EventBody::Midi(MidiData::NoteOn {
            channel,
            note,
            velocity,
        }) => (
            0x90 | u32::from(channel & 0x0F),
            u32::from(note & 0x7F),
            u32::from(velocity & 0x7F),
        ),
        EventBody::Midi(MidiData::NoteOff {
            channel,
            note,
            velocity,
        }) => (
            0x80 | u32::from(channel & 0x0F),
            u32::from(note & 0x7F),
            u32::from(velocity & 0x7F),
        ),
        EventBody::Midi(MidiData::ControlChange {
            channel,
            controller,
            value,
        }) => (
            0xB0 | u32::from(channel & 0x0F),
            u32::from(controller & 0x7F),
            u32::from(value & 0x7F),
        ),
        EventBody::Midi(MidiData::ProgramChange { channel, program }) => (
            0xC0 | u32::from(channel & 0x0F),
            u32::from(program & 0x7F),
            0,
        ),
        EventBody::Midi(MidiData::ChannelAftertouch { channel, pressure }) => (
            0xD0 | u32::from(channel & 0x0F),
            u32::from(pressure & 0x7F),
            0,
        ),
        EventBody::Midi(MidiData::PolyAftertouch {
            channel,
            note,
            pressure,
        }) => (
            0xA0 | u32::from(channel & 0x0F),
            u32::from(note & 0x7F),
            u32::from(pressure & 0x7F),
        ),
        EventBody::Midi(MidiData::PitchBend { channel, value }) => (
            0xE0 | u32::from(channel & 0x0F),
            u32::from(value & 0x7F),
            u32::from((value >> 7) & 0x7F),
        ),
        EventBody::Midi(MidiData::Raw { len, data }) if len >= 1 => {
            let s = u32::from(data[0]);
            let d1 = if len >= 2 { u32::from(data[1]) } else { 0 };
            let d2 = if len >= 3 { u32::from(data[2]) } else { 0 };
            (s, d1, d2)
        }
        _ => return,
    };
    // Non-MusicDevice / non-MIDIFX components return
    // kAudioUnitErr_InvalidProperty here; ignore.
    let _ = unsafe { MusicDeviceMIDIEvent(unit, status, d1, d2, offset) };
}

impl Plugin<f32> for AuPlugin {
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
        let frames_u32 = u32::try_from(frames).unwrap_or(u32::MAX);

        // Refresh the transport snapshot the AU reads back through
        // its host callbacks during AudioUnitRender.
        let sample_time = self.sample_time;
        if let Some(ht) = self.host_transport.as_deref_mut() {
            update_host_transport(ht, context.transport, context.sample_rate, sample_time);
        }

        // Feed MIDI to the AU before AudioUnitRender so the
        // plugin sees the events at the correct sample offset
        // within this block. MusicDeviceMIDIEvent works for both
        // MusicDevice (instruments) and AUMIDIFX effects that
        // accept MIDI; for plain audio effects the call no-ops
        // with `kAudioUnitErr_InvalidProperty`, which we ignore.
        for event in events {
            send_midi(self.instance, event);
        }

        // Refresh the render context's input pointers so the
        // callback hands the AU the right plane addresses for this
        // block.
        let main_inputs = buffer.main_inputs();
        if let Some(ctx) = self.render_ctx.as_deref_mut() {
            ctx.frames = frames;
            ctx.input_planes.clear();
            for chan in main_inputs.iter().take(self.input_channels as usize) {
                ctx.input_planes.push(chan.as_ptr());
            }
            while ctx.input_planes.len() < self.input_channels as usize {
                ctx.input_planes.push(ptr::null());
            }
        }

        // Build the output AudioBufferList in-place over our
        // pre-allocated bytes. Each entry's `mData` points directly
        // at the corresponding notwig-rack-core output channel.
        let main_outputs = buffer.main_outputs();
        // SAFETY: alloc_audio_buffer_list returned a Vec<u8>
        // sized to hold an AudioBufferList plus its variable
        // tail; the Vec is heap-allocated to 8-byte alignment via
        // the global allocator, which suffices for AudioBufferList.
        #[allow(clippy::cast_ptr_alignment)]
        let list_ptr = self
            .output_buffer_storage
            .as_mut_ptr()
            .cast::<AudioBufferList>();
        unsafe {
            (*list_ptr).mNumberBuffers = self.output_channels;
            let buffers_start: *mut CAAudioBuffer = (*list_ptr).mBuffers.as_mut_ptr();
            for ch in 0..self.output_channels as usize {
                let buf = &mut *buffers_start.add(ch);
                buf.mNumberChannels = 1;
                buf.mDataByteSize = u32::try_from(frames * std::mem::size_of::<f32>()).unwrap_or(0);
                buf.mData = main_outputs
                    .get_mut(ch)
                    .map_or(ptr::null_mut(), |c| c.as_mut_ptr().cast());
            }
        }

        let mut flags = AudioUnitRenderActionFlags::empty();
        let timestamp = AudioTimeStamp {
            mSampleTime: self.sample_time,
            mHostTime: 0,
            mRateScalar: 1.0,
            mWordClockTime: 0,
            mSMPTETime: objc2_core_audio_types::SMPTETime {
                mSubframes: 0,
                mSubframeDivisor: 0,
                mCounter: 0,
                mType: objc2_core_audio_types::SMPTETimeType(0),
                mFlags: objc2_core_audio_types::SMPTETimeFlags(0),
                mHours: 0,
                mMinutes: 0,
                mSeconds: 0,
                mFrames: 0,
            },
            mFlags: AudioTimeStampFlags::SampleTimeValid,
            mReserved: 0,
        };
        let status = unsafe {
            AudioUnitRender(
                self.instance,
                &raw mut flags,
                ptr::NonNull::new_unchecked((&raw const timestamp).cast_mut()),
                0, // output bus 0
                frames_u32,
                ptr::NonNull::new_unchecked(list_ptr),
            )
        };
        #[allow(clippy::cast_precision_loss)]
        let frames_f = frames as f64;
        self.sample_time += frames_f;
        if status != 0 {
            // AudioUnitRender may not have touched the output planes on error.
            // Clear them so stale audio is never treated as the block result.
            for channel in buffer.main_outputs() {
                channel.fill(0.0);
            }
            return Ok(ProcessStatus::Error);
        }
        Ok(ProcessStatus::Continue)
    }
}
