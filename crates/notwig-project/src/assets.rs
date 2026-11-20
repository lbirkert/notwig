use std::path::{Path, PathBuf};

use notwig_core::{AssetEntry, AssetId, AssetKind, AudioAssetBuffer};
use sha2::{Digest, Sha256};

use crate::error::ProjectError;
use crate::fsutil;

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
    let bytes = std::fs::read(path)?;
    decode_audio_bytes_with_extension(&extension, bytes)
}

pub fn decode_audio_bytes(name: &str, bytes: Vec<u8>) -> Result<AudioAssetBuffer, ProjectError> {
    let extension = name
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
        .to_ascii_lowercase();
    decode_audio_bytes_with_extension(&extension, bytes)
}

fn decode_audio_bytes_with_extension(
    extension: &str,
    bytes: Vec<u8>,
) -> Result<AudioAssetBuffer, ProjectError> {
    match extension {
        "wav" | "wave" | "aif" | "aiff" | "aifc" | "mp3" | "flac" | "ogg" | "oga" => {
            decode_symphonia(extension, bytes)
        }
        other => Err(ProjectError::UnsupportedAudio(format!(
            "unsupported extension '{other}'"
        ))),
    }
}

struct MemorySource {
    cursor: std::io::Cursor<Vec<u8>>,
}

impl MemorySource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            cursor: std::io::Cursor::new(bytes),
        }
    }
}

impl std::io::Read for MemorySource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl std::io::Seek for MemorySource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl symphonia::core::io::MediaSource for MemorySource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.cursor.get_ref().len() as u64)
    }
}

fn decode_symphonia(extension: &str, bytes: Vec<u8>) -> Result<AudioAssetBuffer, ProjectError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let stream = MediaSourceStream::new(
        Box::new(MemorySource::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|err| ProjectError::Decode(format!("probe: {err}")))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| ProjectError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let params = track.codec_params.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(|err| ProjectError::Decode(format!("decoder: {err}")))?;
    let mut samples = Vec::new();
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut channels = params
        .channels
        .map_or(0, symphonia::core::audio::Channels::count);
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(ProjectError::Decode(
                    "decoder reset required; chained streams are unsupported".into(),
                ));
            }
            Err(err) => return Err(ProjectError::Decode(format!("packet: {err}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let frame = match decoder.decode(&packet) {
            Ok(frame) => frame,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(err) => return Err(ProjectError::Decode(format!("decode: {err}"))),
        };
        let spec = *frame.spec();
        sample_rate = spec.rate;
        channels = spec.channels.count();
        let mut buffer = SampleBuffer::<f32>::new(frame.capacity() as u64, spec);
        buffer.copy_interleaved_ref(frame);
        samples.extend_from_slice(buffer.samples());
    }
    if sample_rate == 0 || channels == 0 {
        return Err(ProjectError::Decode(
            "empty or unsupported audio stream".into(),
        ));
    }
    Ok(AudioAssetBuffer {
        sample_rate,
        channels: u16::try_from(channels)
            .map_err(|_| ProjectError::UnsupportedAudio("too many channels".into()))?,
        samples,
    })
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
    let mut weights = Vec::with_capacity((radius * 2 + 1) as usize);
    for out_frame in 0..out_frames {
        let position = out_frame as f64 / ratio;
        let center = position.floor() as isize;
        weights.clear();
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

pub fn write_wav_f32_bytes(
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<Vec<u8>, ProjectError> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|err| ProjectError::Io(err.to_string()))?;
        for sample in samples {
            writer
                .write_sample(*sample)
                .map_err(|err| ProjectError::Io(err.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|err| ProjectError::Io(err.to_string()))?;
    }
    Ok(cursor.into_inner())
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
    fsutil::ensure_dir(&assets_dir)?;
    let temporary = assets_dir.join(format!("{asset_id}.wav.tmp"));
    write_wav_f32(&temporary, project_rate, channels, &buffer.samples)?;
    let hash = hash_file(&temporary)?;
    let file_name = format!("{hash}.wav");
    let final_path = assets_dir.join(&file_name);
    fsutil::finalize_atomic(&temporary, &final_path)?;
    let uri = PathBuf::from("assets").join(file_name);
    let entry = AssetEntry {
        id: asset_id,
        kind: AssetKind::Audio,
        uri: uri.to_string_lossy().into_owned(),
        original_name: source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
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
