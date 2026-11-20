use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioProbe {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn probe_audio(path: &Path) -> Result<AudioProbe, ProjectError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "wav" | "wave" => probe_wav(path),
        "aif" | "aiff" | "aifc" => probe_aiff(path),
        "flac" => probe_flac(path),
        "mp3" => probe_mp3(path),
        "ogg" | "oga" => probe_ogg(path),
        other => Err(ProjectError::UnsupportedAudio(format!(
            "unsupported extension '{other}'"
        ))),
    }
}

fn probe_wav(path: &Path) -> Result<AudioProbe, ProjectError> {
    let reader =
        hound::WavReader::open(path).map_err(|err| ProjectError::Decode(err.to_string()))?;
    let spec = reader.spec();
    let frames = u64::from(reader.duration());
    if spec.sample_rate == 0 {
        return Err(ProjectError::Decode("WAV has no sample rate".into()));
    }
    Ok(AudioProbe {
        duration_seconds: frames as f64 / f64::from(spec.sample_rate),
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

fn probe_aiff(path: &Path) -> Result<AudioProbe, ProjectError> {
    let mut file = File::open(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let mut header = [0_u8; 12];
    file.read_exact(&mut header)
        .map_err(|err| ProjectError::Decode(err.to_string()))?;
    if &header[0..4] != b"FORM" || &header[8..12] != b"AIFF" {
        return Err(ProjectError::Decode("not an AIFF file".into()));
    }
    loop {
        let mut chunk = [0_u8; 8];
        if file.read_exact(&mut chunk).is_err() {
            break;
        }
        let size = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        if &chunk[0..4] == b"COMM" {
            let mut comm = [0_u8; 18];
            file.read_exact(&mut comm)
                .map_err(|err| ProjectError::Decode(err.to_string()))?;
            let channels = u16::from_be_bytes([comm[0], comm[1]]);
            let frames = u32::from_be_bytes([comm[2], comm[3], comm[4], comm[5]]);
            let rate_bytes: [u8; 10] = comm[8..18].try_into().unwrap_or([0; 10]);
            let sample_rate = extended_to_f64(&rate_bytes);
            if sample_rate <= 0.0 {
                return Err(ProjectError::Decode("AIFF has no sample rate".into()));
            }
            return Ok(AudioProbe {
                duration_seconds: f64::from(frames) / sample_rate,
                sample_rate: sample_rate.round() as u32,
                channels,
            });
        }
        file.seek(SeekFrom::Current(i64::from(size) + i64::from(size % 2)))
            .map_err(|err| ProjectError::Io(err.to_string()))?;
    }
    Err(ProjectError::Decode("AIFF has no COMM chunk".into()))
}

fn extended_to_f64(bytes: &[u8; 10]) -> f64 {
    let sign = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
    let exponent = (i32::from(bytes[0] & 0x7f) << 8) | i32::from(bytes[1]);
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    if exponent == 0 && mantissa == 0 {
        return 0.0;
    }
    sign * (mantissa as f64 / (1_u64 << 63) as f64) * 2.0_f64.powi(exponent - 16383)
}

fn probe_flac(path: &Path) -> Result<AudioProbe, ProjectError> {
    let mut file = File::open(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let mut header = [0_u8; 4];
    file.read_exact(&mut header)
        .map_err(|err| ProjectError::Decode(err.to_string()))?;
    if &header != b"fLaC" {
        return Err(ProjectError::Decode("not a FLAC file".into()));
    }
    loop {
        let mut meta = [0_u8; 4];
        file.read_exact(&mut meta)
            .map_err(|err| ProjectError::Decode(err.to_string()))?;
        let is_last = meta[0] & 0x80 != 0;
        let block_type = meta[0] & 0x7f;
        let length = (u32::from(meta[1]) << 16) | (u32::from(meta[2]) << 8) | u32::from(meta[3]);
        if block_type == 0 {
            let mut info = [0_u8; 34];
            file.read_exact(&mut info)
                .map_err(|err| ProjectError::Decode(err.to_string()))?;
            let sample_rate = (u32::from(info[10]) << 12)
                | (u32::from(info[11]) << 4)
                | (u32::from(info[12]) >> 4);
            let channels = u16::from(((info[12] >> 1) & 0x07) + 1);
            let total_samples = (u64::from(info[13] & 0x0f) << 32)
                | u64::from(u32::from_be_bytes([info[14], info[15], info[16], info[17]]));
            if sample_rate == 0 {
                return Err(ProjectError::Decode("FLAC has no sample rate".into()));
            }
            if total_samples == 0 {
                return Err(ProjectError::Decode(
                    "FLAC has no total sample count".into(),
                ));
            }
            return Ok(AudioProbe {
                duration_seconds: total_samples as f64 / f64::from(sample_rate),
                sample_rate,
                channels,
            });
        }
        file.seek(SeekFrom::Current(i64::from(length)))
            .map_err(|err| ProjectError::Io(err.to_string()))?;
        if is_last {
            break;
        }
    }
    Err(ProjectError::Decode("FLAC has no STREAMINFO block".into()))
}

fn probe_mp3(path: &Path) -> Result<AudioProbe, ProjectError> {
    let mut file = File::open(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let file_size = file
        .metadata()
        .map_err(|err| ProjectError::Io(err.to_string()))?
        .len();
    let mut head = vec![0_u8; 10];
    file.read_exact(&mut head)
        .map_err(|err| ProjectError::Decode(err.to_string()))?;
    let mut audio_start = 0_u64;
    if &head[0..3] == b"ID3" {
        let size = (u32::from(head[6] & 0x7f) << 21)
            | (u32::from(head[7] & 0x7f) << 14)
            | (u32::from(head[8] & 0x7f) << 7)
            | u32::from(head[9] & 0x7f);
        audio_start = 10 + u64::from(size);
    }
    file.seek(SeekFrom::Start(audio_start))
        .map_err(|err| ProjectError::Io(err.to_string()))?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let read = file
        .read(&mut buffer)
        .map_err(|err| ProjectError::Io(err.to_string()))?;
    let buffer = &buffer[..read];
    let Some((frame_start, header)) = find_mp3_frame(buffer) else {
        return Err(ProjectError::Decode("MP3 frame header not found".into()));
    };
    let channels = if header.channel_mode == 3 { 1 } else { 2 };
    let side_info = match (header.version, channels) {
        (1, 1) => 17,
        (1, _) => 32,
        (_, 1) => 9,
        (_, _) => 17,
    };
    let tag_offset = frame_start + 4 + side_info;
    if buffer.len() >= tag_offset + 12
        && (&buffer[tag_offset..tag_offset + 4] == b"Xing"
            || &buffer[tag_offset..tag_offset + 4] == b"Info")
    {
        let flags = u32::from_be_bytes([
            buffer[tag_offset + 4],
            buffer[tag_offset + 5],
            buffer[tag_offset + 6],
            buffer[tag_offset + 7],
        ]);
        if flags & 1 != 0 {
            let frames = u32::from_be_bytes([
                buffer[tag_offset + 8],
                buffer[tag_offset + 9],
                buffer[tag_offset + 10],
                buffer[tag_offset + 11],
            ]);
            let samples_per_frame = if header.version == 1 { 1152 } else { 576 };
            return Ok(AudioProbe {
                duration_seconds: f64::from(frames) * f64::from(samples_per_frame)
                    / f64::from(header.sample_rate),
                sample_rate: header.sample_rate,
                channels,
            });
        }
    }
    let audio_bytes = file_size
        .saturating_sub(audio_start)
        .saturating_sub(if file_size >= 128 { 128 } else { 0 });
    let duration_seconds = if header.bitrate_kbps == 0 {
        0.0
    } else {
        audio_bytes as f64 * 8.0 / (f64::from(header.bitrate_kbps) * 1000.0)
    };
    if duration_seconds <= 0.0 {
        return Err(ProjectError::Decode("MP3 duration is unavailable".into()));
    }
    Ok(AudioProbe {
        duration_seconds,
        sample_rate: header.sample_rate,
        channels,
    })
}

struct Mp3Header {
    version: u8,
    sample_rate: u32,
    bitrate_kbps: u32,
    channel_mode: u8,
}

fn find_mp3_frame(buffer: &[u8]) -> Option<(usize, Mp3Header)> {
    for index in 0..buffer.len().saturating_sub(4) {
        if buffer[index] != 0xff || buffer[index + 1] & 0xe0 != 0xe0 {
            continue;
        }
        let b1 = buffer[index + 1];
        let b2 = buffer[index + 2];
        let b3 = buffer[index + 3];
        let version = match (b1 >> 3) & 0x03 {
            3 => 1,
            2 => 2,
            0 => 25,
            _ => continue,
        };
        let layer = (b1 >> 1) & 0x03;
        if layer != 1 {
            continue;
        }
        let bitrate_index = (b2 >> 4) & 0x0f;
        let rate_index = (b2 >> 2) & 0x03;
        let bitrate_kbps = mp3_bitrate(version, bitrate_index)?;
        let sample_rate = match (version, rate_index) {
            (1, 0) => 44_100,
            (1, 1) => 48_000,
            (1, 2) => 32_000,
            (2, 0) => 22_050,
            (2, 1) => 24_000,
            (2, 2) => 16_000,
            (25, 0) => 11_025,
            (25, 1) => 12_000,
            (25, 2) => 8_000,
            _ => continue,
        };
        let channel_mode = (b3 >> 6) & 0x03;
        return Some((
            index,
            Mp3Header {
                version,
                sample_rate,
                bitrate_kbps,
                channel_mode,
            },
        ));
    }
    None
}

fn mp3_bitrate(version: u8, index: u8) -> Option<u32> {
    let table_v1 = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    let table_v2 = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    let table = if version == 1 { table_v1 } else { table_v2 };
    table
        .get(usize::from(index))
        .copied()
        .filter(|value| *value > 0)
}

fn probe_ogg(path: &Path) -> Result<AudioProbe, ProjectError> {
    let mut file = File::open(path).map_err(|err| ProjectError::Io(err.to_string()))?;
    let file_size = file
        .metadata()
        .map_err(|err| ProjectError::Io(err.to_string()))?
        .len();
    let mut head = vec![0_u8; (64 * 1024).min(file_size as usize)];
    file.read_exact(&mut head)
        .map_err(|err| ProjectError::Decode(err.to_string()))?;
    let Some(sample_rate) = ogg_vorbis_rate(&head) else {
        return Err(ProjectError::Decode("Ogg Vorbis rate not found".into()));
    };
    let tail_len = (64 * 1024).min(file_size as usize);
    let mut tail = vec![0_u8; tail_len];
    file.seek(SeekFrom::Start(file_size - tail_len as u64))
        .map_err(|err| ProjectError::Io(err.to_string()))?;
    file.read_exact(&mut tail)
        .map_err(|err| ProjectError::Decode(err.to_string()))?;
    let Some(granule) = ogg_last_granule(&tail) else {
        return Err(ProjectError::Decode("Ogg granule not found".into()));
    };
    if sample_rate == 0 {
        return Err(ProjectError::Decode("Ogg has no sample rate".into()));
    }
    Ok(AudioProbe {
        duration_seconds: granule as f64 / f64::from(sample_rate),
        sample_rate,
        channels: ogg_vorbis_channels(&head).unwrap_or(2),
    })
}

fn ogg_vorbis_rate(bytes: &[u8]) -> Option<u32> {
    let start = find_subslice(bytes, b"vorbis")?;
    let rate_offset = start + 6 + 4 + 1;
    bytes
        .get(rate_offset..rate_offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn ogg_vorbis_channels(bytes: &[u8]) -> Option<u16> {
    let start = find_subslice(bytes, b"vorbis")?;
    bytes.get(start + 6 + 4).map(|value| u16::from(*value))
}

fn ogg_last_granule(bytes: &[u8]) -> Option<u64> {
    for index in (0..bytes.len().saturating_sub(14)).rev() {
        if &bytes[index..index + 4] == b"OggS" {
            let granule = u64::from_le_bytes([
                bytes[index + 6],
                bytes[index + 7],
                bytes[index + 8],
                bytes[index + 9],
                bytes[index + 10],
                bytes[index + 11],
                bytes[index + 12],
                bytes[index + 13],
            ]);
            if granule != u64::MAX {
                return Some(granule);
            }
        }
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
