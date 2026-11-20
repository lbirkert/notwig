use std::io::Write;
use std::path::Path;

use flacenc::component::BitRepr;
use flacenc::error::Verify;

use crate::error::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Wav,
    Aiff,
    Flac,
    Mp3,
    Ogg,
}

impl ExportFormat {
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBitDepth {
    Int16,
    Int24,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub bit_depth: ExportBitDepth,
    pub dither: bool,
    pub normalize: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Wav,
            bit_depth: ExportBitDepth::Float32,
            dither: false,
            normalize: false,
        }
    }
}

pub fn write_audio_export(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
    options: ExportOptions,
) -> Result<(), ProjectError> {
    let mut prepared = samples.to_vec();
    if options.normalize {
        normalize(&mut prepared);
    }
    match options.format {
        ExportFormat::Wav => write_wav(path, sample_rate, channels, &prepared, options),
        ExportFormat::Aiff => write_aiff(path, sample_rate, channels, &prepared, options),
        ExportFormat::Flac => write_flac(path, sample_rate, channels, &prepared, options),
        ExportFormat::Mp3 => write_mp3(path, sample_rate, channels, &prepared),
        ExportFormat::Ogg => write_ogg(path, sample_rate, channels, &prepared),
    }
}

fn normalize(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak > 0.0 {
        let gain = 0.999 / peak;
        for sample in samples {
            *sample *= gain;
        }
    }
}

fn write_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
    options: ExportOptions,
) -> Result<(), ProjectError> {
    let (bits, format) = match options.bit_depth {
        ExportBitDepth::Int16 => (16, hound::SampleFormat::Int),
        ExportBitDepth::Int24 => (24, hound::SampleFormat::Int),
        ExportBitDepth::Float32 => (32, hound::SampleFormat::Float),
    };
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: bits,
        sample_format: format,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|err| ProjectError::Io(err.to_string()))?;
    match options.bit_depth {
        ExportBitDepth::Int16 => {
            for (index, sample) in samples.iter().enumerate() {
                let value = quantize(*sample, 16, options.dither, index);
                writer
                    .write_sample(value as i16)
                    .map_err(|err| ProjectError::Io(err.to_string()))?;
            }
        }
        ExportBitDepth::Int24 => {
            for (index, sample) in samples.iter().enumerate() {
                let value = quantize(*sample, 24, options.dither, index);
                writer
                    .write_sample(value)
                    .map_err(|err| ProjectError::Io(err.to_string()))?;
            }
        }
        ExportBitDepth::Float32 => {
            for sample in samples {
                writer
                    .write_sample(sample.clamp(-1.0, 1.0))
                    .map_err(|err| ProjectError::Io(err.to_string()))?;
            }
        }
    }
    writer
        .finalize()
        .map_err(|err| ProjectError::Io(err.to_string()))
}

fn write_aiff(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
    options: ExportOptions,
) -> Result<(), ProjectError> {
    let bits: u32 = match options.bit_depth {
        ExportBitDepth::Int16 => 16,
        ExportBitDepth::Int24 => 24,
        ExportBitDepth::Float32 => 32,
    };
    let frames = samples.len() / usize::from(channels.max(1));
    let bytes_per_sample = bits / 8;
    let mut comm = Vec::new();
    comm.extend_from_slice(&channels.to_be_bytes());
    comm.extend_from_slice(
        &u32::try_from(frames)
            .map_err(|_| ProjectError::Io("audio is too long for AIFF".into()))?
            .to_be_bytes(),
    );
    comm.extend_from_slice(&u16::try_from(bits).unwrap_or(0).to_be_bytes());
    comm.extend_from_slice(&extended_sample_rate(sample_rate));

    let mut data = Vec::with_capacity(samples.len() * bytes_per_sample as usize);
    for (index, sample) in samples.iter().enumerate() {
        let value = quantize(*sample, bits, options.dither, index);
        match bits {
            16 => data.extend_from_slice(&(value as i16).to_be_bytes()),
            24 => {
                let bytes = value.to_be_bytes();
                data.extend_from_slice(&bytes[1..4]);
            }
            _ => data.extend_from_slice(&value.to_be_bytes()),
        }
    }

    let mut body = Vec::new();
    body.extend_from_slice(b"AIFF");
    write_chunk(&mut body, *b"COMM", &comm);
    let mut ssnd = Vec::new();
    ssnd.extend_from_slice(&0_u32.to_be_bytes());
    ssnd.extend_from_slice(&0_u32.to_be_bytes());
    ssnd.extend_from_slice(&data);
    write_chunk(&mut body, *b"SSND", &ssnd);

    let mut file = std::fs::File::create(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let form_len = u32::try_from(body.len())
        .map_err(|_| ProjectError::Io("audio is too long for AIFF".into()))?;
    file.write_all(b"FORM")
        .and_then(|()| file.write_all(&form_len.to_be_bytes()))
        .and_then(|()| file.write_all(&body))
        .map_err(|err| ProjectError::Io(err.to_string()))?;
    Ok(())
}

fn write_flac(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
    options: ExportOptions,
) -> Result<(), ProjectError> {
    let bits = match options.bit_depth {
        ExportBitDepth::Int16 => 16,
        ExportBitDepth::Int24 | ExportBitDepth::Float32 => 24,
    };
    let pcm: Vec<i32> = samples
        .iter()
        .enumerate()
        .map(|(index, sample)| quantize(*sample, bits, options.dither, index))
        .collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, err)| ProjectError::Encode(err.to_string()))?;
    let source = flacenc::source::MemSource::from_samples(
        &pcm,
        usize::from(channels.max(1)),
        bits as usize,
        sample_rate as usize,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|err| ProjectError::Encode(err.to_string()))?;
    let block_size = config.block_size;
    stream
        .stream_info_mut()
        .set_block_sizes(block_size, block_size)
        .map_err(|err| ProjectError::Encode(err.to_string()))?;
    let frames = samples.len() / usize::from(channels.max(1));
    stream.stream_info_mut().set_total_samples(frames);
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|err| ProjectError::Encode(err.to_string()))?;
    std::fs::write(path, sink.as_slice()).map_err(|err| ProjectError::Io(err.to_string()))
}

fn write_mp3(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<(), ProjectError> {
    let mut encoder = rusty_mp3::Mp3Encoder::new(rusty_mp3::Mp3EncoderConfig {
        bitrate_kbps: 192,
        vbr_quality: None,
    });
    encoder
        .push_pcm_f32(samples, channels.max(1), sample_rate)
        .map_err(|err| ProjectError::Encode(err.to_string()))?;
    encoder.finish();
    let mut output = Vec::new();
    loop {
        match encoder.next_packet() {
            Ok(packet) => output.extend_from_slice(&packet),
            Err(rusty_mp3::Error::Eof | rusty_mp3::Error::Again) => break,
            Err(err) => return Err(ProjectError::Encode(err.to_string())),
        }
    }
    std::fs::write(path, output).map_err(|err| ProjectError::Io(err.to_string()))
}

fn write_ogg(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<(), ProjectError> {
    let mut encoder = rusty_vorbis::VorbisEncoder::new(rusty_vorbis::VorbisEncoderConfig {
        quality: rusty_vorbis::quality01_from_vorbis_q(5.0),
        ..rusty_vorbis::VorbisEncoderConfig::default()
    });
    let (pcm, channels) = if channels == 1 {
        (duplicate_mono(samples), 2)
    } else {
        (samples.to_vec(), channels.max(1))
    };
    encoder
        .push_pcm_f32(&pcm, channels, sample_rate)
        .map_err(|err| ProjectError::Encode(err.to_string()))?;
    encoder.finish();
    let mut packets = Vec::new();
    loop {
        match encoder.next_packet() {
            Ok(packet) => packets.push(packet),
            Err(rusty_vorbis::Error::Eof | rusty_vorbis::Error::Again) => break,
            Err(err) => return Err(ProjectError::Encode(err.to_string())),
        }
    }
    let file = std::fs::File::create(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let mut writer = ogg::PacketWriter::new(file);
    let serial = 0x4e6f_7477;
    let last = packets.len().saturating_sub(1);
    for (index, packet) in packets.iter().enumerate() {
        let end = if index == 0 || index == 2 {
            ogg::PacketWriteEndInfo::EndPage
        } else if index == last {
            ogg::PacketWriteEndInfo::EndStream
        } else {
            ogg::PacketWriteEndInfo::NormalPacket
        };
        writer
            .write_packet(packet.data.clone(), serial, end, packet.pts.max(0) as u64)
            .map_err(|err| ProjectError::Io(err.to_string()))?;
    }
    Ok(())
}

fn write_chunk(body: &mut Vec<u8>, id: [u8; 4], data: &[u8]) {
    body.extend_from_slice(&id);
    body.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    body.extend_from_slice(data);
    if data.len() % 2 == 1 {
        body.push(0);
    }
}

fn extended_sample_rate(sample_rate: u32) -> [u8; 10] {
    let value = f64::from(sample_rate);
    if value <= 0.0 {
        return [0; 10];
    }
    let bits = value.to_bits();
    let mut exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mut fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == -1023 {
        exponent += 1;
    } else {
        fraction |= 1 << 52;
    }
    let ext_exponent = u16::try_from(exponent + 16383).unwrap_or(0);
    let mantissa = fraction << 11;
    let mut out = [0_u8; 10];
    out[0..2].copy_from_slice(&ext_exponent.to_be_bytes());
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

fn quantize(sample: f32, bits: u32, dither: bool, index: usize) -> i32 {
    let levels = 1_i64 << (bits - 1);
    let level_count = levels as f32;
    let mut value = sample.clamp(-1.0, 1.0);
    if dither {
        let noise = (noise_at(index * 2) - noise_at(index * 2 + 1)) / level_count;
        value += noise;
    }
    let scaled = (value * level_count).round() as i64;
    i32::try_from(scaled.clamp(-levels, levels - 1)).unwrap_or(0)
}

fn noise_at(index: usize) -> f32 {
    let mut state = (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    state ^= state >> 30;
    state = state.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    state ^= state >> 27;
    state = state.wrapping_mul(0x94d0_49bb_1331_11eb);
    state ^= state >> 31;
    (state >> 40) as f32 / (1_u32 << 24) as f32
}

fn duplicate_mono(samples: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.push(*sample);
        out.push(*sample);
    }
    out
}
