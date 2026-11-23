use objc2_audio_toolbox::AudioUnitRenderActionFlags;
use objc2_core_audio_types::{AudioBuffer as CAAudioBuffer, AudioBufferList, AudioTimeStamp};

/// Audio-thread state shared with the AU render callback. The
/// callback fills the AU's input buffer from `input_planes` —
/// `process()` refreshes these pointers each block.
pub(crate) struct RenderContext {
    /// One pointer per channel into the host's input data for the
    /// current block. NULL when the plugin doesn't take input
    /// (instrument case).
    pub(crate) input_planes: Vec<*const f32>,
    /// Frames per block — must match `inNumberFrames` the AU
    /// passes to the callback. We assert and zero-fill on mismatch.
    pub(crate) frames: usize,
}

pub(crate) unsafe extern "C-unwind" fn render_input_callback(
    in_ref_con: std::ptr::NonNull<std::ffi::c_void>,
    _io_action_flags: std::ptr::NonNull<AudioUnitRenderActionFlags>,
    _in_time_stamp: std::ptr::NonNull<AudioTimeStamp>,
    _in_bus_number: u32,
    in_number_frames: u32,
    io_data: *mut AudioBufferList,
) -> i32 {
    if io_data.is_null() {
        return -1;
    }
    let ctx_ptr = in_ref_con.as_ptr().cast::<RenderContext>();
    let ctx = unsafe { &*ctx_ptr };
    let list = unsafe { &mut *io_data };
    let frames = in_number_frames as usize;
    let buffer_count = list.mNumberBuffers as usize;
    // The variable-length tail is reached via mBuffers as a [_; 1]
    // declared slice we extend via pointer arithmetic.
    let buffers_ptr: *mut CAAudioBuffer = list.mBuffers.as_mut_ptr();
    let bytes_u32 = u32::try_from(frames * std::mem::size_of::<f32>()).unwrap_or(0);
    let bytes_usize = bytes_u32 as usize;
    for i in 0..buffer_count {
        let buffer = unsafe { &mut *buffers_ptr.add(i) };
        buffer.mDataByteSize = bytes_u32;
        if let Some(&plane) = ctx.input_planes.get(i)
            && !plane.is_null()
            && frames <= ctx.frames
        {
            buffer.mData = plane.cast::<std::ffi::c_void>().cast_mut();
            continue;
        }
        // No input available for this channel — leave mData where
        // the caller put it and zero whatever bytes were there.
        if !buffer.mData.is_null() {
            unsafe {
                std::ptr::write_bytes(buffer.mData.cast::<u8>(), 0, bytes_usize);
            }
        }
    }
    0
}

/// Allocate enough bytes to hold an `AudioBufferList` with
/// `n_buffers` `AudioBuffer`s tail-included. Returns a `Vec<u8>`
/// large enough; the caller casts the data pointer.
pub(crate) fn alloc_audio_buffer_list(n_buffers: usize) -> Vec<u8> {
    let n = n_buffers.max(1);
    let size =
        std::mem::size_of::<AudioBufferList>() + (n - 1) * std::mem::size_of::<CAAudioBuffer>();
    vec![0u8; size]
}
