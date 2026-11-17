use std::path::{Path, PathBuf};

use notwig_core::{AssetEntry, AssetId, AssetKind, AudioAssetBuffer};
use sha2::{Digest, Sha256};

use crate::error::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleQuality {
    Standard,
    High,
}

#[derive(Debug, Clone)]
pub struct ImportedAsset {
    pub entry: AssetEntry,
    pub buffer: AudioAssetBuffer,
}

pub fn decode_audio(path: &Path) -> Result<AudioAssetBuffer, ProjectError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "wav" | "wave" => decode_wav(path),
        "aif" | "aiff" | "aifc" => decode_aiff(path),
        other => Err(ProjectError::UnsupportedAudio(format!(
            "unsupported extension '{other}'"
        ))),
    }
}

fn decode_wav(path: &Path) -> Result<AudioAssetBuffer, ProjectError> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|err| ProjectError::Decode(format!("wav open: {err}")))?;
    let spec = reader.spec();
    let channels = spec.channels;
    let samples: Vec<f32> = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ProjectError::Decode(format!("wav samples: {err}")))?;
    Ok(AudioAssetBuffer {
        sample_rate: spec.sample_rate,
        channels,
        samples,
    })
}

fn decode_aiff(path: &Path) -> Result<AudioAssetBuffer, ProjectError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"FORM" {
        return Err(ProjectError::Decode("not an AIFF file".into()));
    }
    let form = &bytes[8..12];
    let is_aifc = form == b"AIFC";
    if form != b"AIFF" && !is_aifc {
        return Err(ProjectError::Decode("unsupported AIFF form".into()));
    }
    let mut offset = 12usize;
    let mut channels = 0u16;
    let mut frames = 0u32;
    let mut bits = 0u16;
    let mut sample_rate = 0u32;
    let mut compression = *b"NONE";
    let mut sound_data: Option<&[u8]> = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let data_start = offset + 8;
        let data_end = data_start.saturating_add(size).min(bytes.len());
        let data = &bytes[data_start..data_end];
        match id {
            b"COMM" => {
                if data.len() < 18 {
                    return Err(ProjectError::Decode("short COMM chunk".into()));
                }
                channels = u16::from_be_bytes(data[0..2].try_into().unwrap());
                frames = u32::from_be_bytes(data[2..6].try_into().unwrap());
                bits = u16::from_be_bytes(data[6..8].try_into().unwrap());
                let rate = read_extended_80(&data[8..18]);
                sample_rate = rate.round() as u32;
                if is_aifc && data.len() >= 22 {
                    compression.copy_from_slice(&data[18..22]);
                }
            }
            b"SSND" => {
                if data.len() < 8 {
                    return Err(ProjectError::Decode("short SSND chunk".into()));
                }
                let data_offset = u32::from_be_bytes(data[0..4].try_into().unwrap()) as usize;
                let start = 8usize.saturating_add(data_offset);
                if start < data.len() {
                    sound_data = Some(&data[start..]);
                }
            }
            _ => {}
        }
        offset = data_end + (size & 1);
    }
    let data = sound_data.ok_or_else(|| ProjectError::Decode("missing SSND chunk".into()))?;
    if channels == 0 || sample_rate == 0 || bits == 0 {
        return Err(ProjectError::Decode("invalid COMM chunk".into()));
    }
    let little_endian = is_aifc && &compression == b"sowt";
    let float = is_aifc && (&compression == b"fl32" || &compression == b"FL32");
    let double = is_aifc && (&compression == b"fl64" || &compression == b"FL64");
    let bytes_per_sample = usize::from(bits / 8);
    if bytes_per_sample == 0 {
        return Err(ProjectError::Decode("invalid sample width".into()));
    }
    let expected = frames as usize * channels as usize * bytes_per_sample;
    if data.len() < expected {
        return Err(ProjectError::Decode("truncated sound data".into()));
    }
    let mut samples = Vec::with_capacity(frames as usize * channels as usize);
    for index in 0..frames as usize * channels as usize {
        let start = index * bytes_per_sample;
        let raw = &data[start..start + bytes_per_sample];
        let value = if float && bytes_per_sample == 4 {
            f32::from_bits(read_u32(raw, little_endian)) as f64
        } else if double && bytes_per_sample == 8 {
            f64::from_bits(read_u64(raw, little_endian))
        } else {
            match bits {
                8 => f64::from(raw[0] as i8) / 128.0,
                16 => {
                    let value = if little_endian {
                        i16::from_le_bytes(raw.try_into().unwrap())
                    } else {
                        i16::from_be_bytes(raw.try_into().unwrap())
                    };
                    f64::from(value) / 32_768.0
                }
                24 => {
                    let mut bytes3 = [0u8; 3];
                    bytes3.copy_from_slice(raw);
                    let value = if little_endian {
                        i32::from_le_bytes([bytes3[0], bytes3[1], bytes3[2], 0]) >> 8
                    } else {
                        i32::from_be_bytes([0, bytes3[0], bytes3[1], bytes3[2]]) >> 8
                    };
                    f64::from(value) / 8_388_608.0
                }
                32 => {
                    let value = if little_endian {
                        i32::from_le_bytes(raw.try_into().unwrap())
                    } else {
                        i32::from_be_bytes(raw.try_into().unwrap())
                    };
                    f64::from(value) / 2_147_483_648.0
                }
                _ => {
                    return Err(ProjectError::UnsupportedAudio(format!(
                        "unsupported AIFF sample width {bits}"
                    )));
                }
            }
        };
        samples.push(value as f32);
    }
    Ok(AudioAssetBuffer {
        sample_rate,
        channels,
        samples,
    })
}

fn read_u32(bytes: &[u8], little_endian: bool) -> u32 {
    if little_endian {
        u32::from_le_bytes(bytes.try_into().unwrap())
    } else {
        u32::from_be_bytes(bytes.try_into().unwrap())
    }
}

fn read_u64(bytes: &[u8], little_endian: bool) -> u64 {
    if little_endian {
        u64::from_le_bytes(bytes.try_into().unwrap())
    } else {
        u64::from_be_bytes(bytes.try_into().unwrap())
    }
}

fn read_extended_80(bytes: &[u8]) -> f64 {
    let sign = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
    let exponent = (((bytes[0] & 0x7f) as i32) << 8) | i32::from(bytes[1]);
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    if exponent == 0 && mantissa == 0 {
        return 0.0;
    }
    sign * (mantissa as f64 / 2.0_f64.powi(63)) * 2.0_f64.powi(exponent - 16383)
}

pub fn resample(
    input: &AudioAssetBuffer,
    target_rate: u32,
    quality: ResampleQuality,
) -> Result<AudioAssetBuffer, ProjectError> {
    if target_rate == 0 {
        return Err(ProjectError::Resample(
            "target rate must be positive".into(),
        ));
    }
    if input.sample_rate == target_rate {
        return Ok(input.clone());
    }
    let channels = usize::from(input.channels.max(1));
    let in_frames = input.samples.len() / channels;
    let ratio = f64::from(target_rate) / f64::from(input.sample_rate);
    let out_frames = (in_frames as f64 * ratio).round() as usize;
    let mut samples = vec![0.0_f32; out_frames * channels];
    let radius = match quality {
        ResampleQuality::Standard => 1,
        ResampleQuality::High => 3,
    };
    for out_frame in 0..out_frames {
        let position = out_frame as f64 / ratio;
        let center = position.floor() as isize;
        let mut weights = Vec::with_capacity((radius * 2 + 1) as usize);
        let mut sum_weight = 0.0_f64;
        for tap in -radius..=radius {
            let distance = position - (center + tap) as f64;
            let weight = if radius == 1 {
                (1.0 - distance.abs()).max(0.0)
            } else {
                sinc(distance) * sinc(distance / radius as f64)
            };
            weights.push((center + tap, weight));
            sum_weight += weight;
        }
        if sum_weight.abs() < 1e-12 {
            continue;
        }
        for channel in 0..channels {
            let mut value = 0.0_f64;
            for (index, weight) in &weights {
                if *index >= 0 && (*index as usize) < in_frames {
                    value +=
                        f64::from(input.samples[*index as usize * channels + channel]) * weight;
                }
            }
            samples[out_frame * channels + channel] = (value / sum_weight) as f32;
        }
    }
    Ok(AudioAssetBuffer {
        sample_rate: target_rate,
        channels: input.channels,
        samples,
    })
}

fn sinc(value: f64) -> f64 {
    if value.abs() < 1e-12 {
        1.0
    } else {
        let x = std::f64::consts::PI * value;
        x.sin() / x
    }
}

pub fn write_wav_f32(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<(), ProjectError> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|err| ProjectError::Io(err.to_string()))?;
    for sample in samples {
        writer
            .write_sample(*sample)
            .map_err(|err| ProjectError::Io(err.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|err| ProjectError::Io(err.to_string()))
}

pub fn import_asset(
    source: &Path,
    bundle: &Path,
    asset_id: AssetId,
    project_rate: u32,
) -> Result<ImportedAsset, ProjectError> {
    let decoded = decode_audio(source)?;
    if decoded.channels > 2 {
        return Err(ProjectError::UnsupportedAudio(
            "more than two channels are not supported".into(),
        ));
    }
    let converted = resample(&decoded, project_rate, ResampleQuality::High)?;
    let channels = if converted.channels == 1 {
        2
    } else {
        converted.channels
    };
    let samples = if converted.channels == 1 {
        duplicate_mono(&converted.samples)
    } else {
        converted.samples.clone()
    };
    let buffer = AudioAssetBuffer {
        sample_rate: project_rate,
        channels,
        samples,
    };
    let assets_dir = bundle.join("assets");
    std::fs::create_dir_all(&assets_dir)?;
    let temporary = assets_dir.join(format!("{}.wav.tmp", asset_id));
    write_wav_f32(&temporary, project_rate, channels, &buffer.samples)?;
    let hash = hash_file(&temporary)?;
    let file_name = format!("{hash}.wav");
    let final_path = assets_dir.join(&file_name);
    std::fs::rename(&temporary, &final_path)?;
    let uri = PathBuf::from("assets").join(file_name);
    let entry = AssetEntry {
        id: asset_id,
        kind: AssetKind::Audio,
        uri: uri.to_string_lossy().into_owned(),
        hash: Some(hash),
        format: "wav".into(),
        sample_rate: Some(project_rate),
        channels: Some(channels),
        duration_samples: Some(buffer.frame_count()),
        original_sample_rate: Some(decoded.sample_rate),
        original_channels: Some(decoded.channels),
    };
    Ok(ImportedAsset { entry, buffer })
}

fn duplicate_mono(samples: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.push(*sample);
        out.push(*sample);
    }
    out
}

fn hash_file(path: &Path) -> Result<String, ProjectError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
