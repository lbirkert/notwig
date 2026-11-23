macro_rules! with_output_sample_format {
    ($format:expr, $ty:ident, $body:expr) => {
        match $format {
            cpal::SampleFormat::F32 => {
                type $ty = f32;
                $body
            }
            cpal::SampleFormat::I16 => {
                type $ty = i16;
                $body
            }
            cpal::SampleFormat::U16 => {
                type $ty = u16;
                $body
            }
            cpal::SampleFormat::I32 => {
                type $ty = i32;
                $body
            }
            other => Err(format!("unsupported sample format {other}")),
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! with_input_sample_format {
    ($format:expr, $ty:ident, $body:expr) => {
        match $format {
            cpal::SampleFormat::F32 => {
                type $ty = f32;
                $body
            }
            cpal::SampleFormat::I16 => {
                type $ty = i16;
                $body
            }
            cpal::SampleFormat::U16 => {
                type $ty = u16;
                $body
            }
            cpal::SampleFormat::I32 => {
                type $ty = i32;
                $body
            }
            cpal::SampleFormat::I8 => {
                type $ty = i8;
                $body
            }
            cpal::SampleFormat::U8 => {
                type $ty = u8;
                $body
            }
            other => Err(format!("unsupported sample format {other}")),
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use with_input_sample_format;
pub(crate) use with_output_sample_format;
