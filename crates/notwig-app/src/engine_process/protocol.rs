use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use notwig_core::dsp::CompiledPlan;
use notwig_core::protocol::PlaybackSchedule;
use notwig_core::{AssetId, AudioAssetBuffer, AudioInputRef, DeviceId, ParamId, TrackId};
use notwig_engine::{EngineConfig, EngineControlUpdate};
use notwig_ext_api::ExtensionSettings;
use notwig_plugins::{PluginDescriptor, PluginState};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::audio::{AudioSettings, AudioSnapshot};

pub(crate) const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME_BYTES: usize = 1024 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct WireBuild {
    pub(crate) config: EngineConfig,
    pub(crate) plan: CompiledPlan,
    pub(crate) schedule: PlaybackSchedule,
    pub(crate) assets: BTreeMap<AssetId, Arc<AudioAssetBuffer>>,
    pub(crate) plugin_descriptors: Vec<PluginDescriptor>,
    pub(crate) plugin_states: BTreeMap<DeviceId, PluginState>,
    pub(crate) extension_root: Option<PathBuf>,
    pub(crate) extension_settings: BTreeMap<String, ExtensionSettings>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum EngineRequest {
    Hello {
        version: u32,
    },
    OpenDevice {
        project_rate: u32,
        settings: AudioSettings,
    },
    Build(WireBuild),
    Control(EngineControlUpdate),
    ConfigureInputs {
        requests: Vec<(TrackId, AudioInputRef)>,
    },
    StartRecording,
    StopRecording,
    OpenEditor {
        device_id: DeviceId,
        descriptor: PluginDescriptor,
        sample_rate: u32,
        state: Option<PluginState>,
        params: Vec<(ParamId, f64)>,
    },
    CloseEditor {
        device_id: DeviceId,
    },
    FocusEditor {
        device_id: DeviceId,
    },
    SetEditorParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: f64,
    },
    Shutdown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum EngineReply {
    Ready {
        version: u32,
    },
    DeviceOpened {
        driver: String,
        device_name: String,
        sample_rate: u32,
        channels: u16,
        block_size: u32,
    },
    Snapshot(Box<AudioSnapshot>),
    BuildFinished(Result<(), String>),
    EditorOpened {
        device_id: DeviceId,
    },
    EditorClosed {
        device_id: DeviceId,
        state: Option<PluginState>,
        error: Option<String>,
    },
    EditorParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: f64,
    },
    EditorTransportToggle,
    InputError(String),
    RecordingFinished(Vec<(TrackId, AudioAssetBuffer)>),
    RecordingFailed(String),
    Error(String),
}

pub(crate) fn write_frame<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let bytes = postcard::to_allocvec(value)
        .map_err(|error| io::Error::other(format!("encode engine frame: {error}")))?;
    let len =
        u32::try_from(bytes.len()).map_err(|_| io::Error::other("engine frame is too large"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

pub(crate) fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<Option<T>> {
    let mut len = [0_u8; 4];
    match reader.read_exact(&mut len) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let len = usize::try_from(u32::from_le_bytes(len))
        .map_err(|_| io::Error::other("invalid engine frame length"))?;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::other("engine frame exceeds size limit"));
    }
    let mut bytes = vec![0_u8; len];
    reader.read_exact(&mut bytes)?;
    let value = postcard::from_bytes(&bytes)
        .map_err(|error| io::Error::other(format!("decode engine frame: {error}")))?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frames_round_trip() {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &EngineRequest::Hello {
                version: PROTOCOL_VERSION,
            },
        )
        .unwrap();
        write_frame(&mut bytes, &EngineReply::BuildFinished(Ok(()))).unwrap();

        let mut reader = Cursor::new(bytes);
        let request: EngineRequest = read_frame(&mut reader).unwrap().unwrap();
        assert!(matches!(request, EngineRequest::Hello { .. }));
        let reply: EngineReply = read_frame(&mut reader).unwrap().unwrap();
        assert!(matches!(reply, EngineReply::BuildFinished(Ok(()))));
        assert!(
            read_frame::<_, EngineRequest>(&mut reader)
                .unwrap()
                .is_none()
        );
    }
}
