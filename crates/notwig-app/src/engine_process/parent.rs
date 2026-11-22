use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use notwig_core::{AudioAssetBuffer, AudioInputRef, DeviceId, ParamId, TrackId};
use notwig_plugins::{PluginDescriptor, PluginState};

use super::protocol::{
    EngineReply, EngineRequest, PROTOCOL_VERSION, WireBuild, read_frame, write_frame,
};
use crate::audio::{AudioService, AudioSettings, AudioSnapshot};

pub(crate) enum EngineEvent {
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

#[derive(Clone)]
pub(crate) struct EngineClientHandle {
    requests: mpsc::Sender<EngineRequest>,
}

impl EngineClientHandle {
    pub(crate) fn send_build(&self, build: WireBuild) -> Result<(), String> {
        self.requests
            .send(EngineRequest::Build(build))
            .map_err(|_| "audio engine process is not running".into())
    }

    pub(crate) fn send_control(
        &self,
        update: notwig_engine::EngineControlUpdate,
    ) -> Result<(), String> {
        self.requests
            .send(EngineRequest::Control(update))
            .map_err(|_| "audio engine process is not running".into())
    }

    pub(crate) fn configure_inputs(
        &self,
        requests: Vec<(TrackId, AudioInputRef)>,
    ) -> Result<(), String> {
        self.requests
            .send(EngineRequest::ConfigureInputs { requests })
            .map_err(|_| "audio engine process is not running".into())
    }

    pub(crate) fn start_recording(&self) -> Result<(), String> {
        self.requests
            .send(EngineRequest::StartRecording)
            .map_err(|_| "audio engine process is not running".into())
    }

    pub(crate) fn stop_recording(&self) -> Result<(), String> {
        self.requests
            .send(EngineRequest::StopRecording)
            .map_err(|_| "audio engine process is not running".into())
    }
}

pub(crate) struct EngineClient {
    snapshot: Arc<Mutex<AudioSnapshot>>,
    requests: mpsc::Sender<EngineRequest>,
    events: mpsc::Receiver<EngineEvent>,
    child: Child,
}

impl EngineClient {
    pub(crate) fn start(project_rate: u32, settings: &AudioSettings) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut child = Command::new(exe)
            .arg("engine-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start engine process: {error}"))?;
        let mut stdin = child.stdin.take().ok_or("engine process has no stdin")?;
        let mut stdout = child.stdout.take().ok_or("engine process has no stdout")?;

        if let Err(error) = write_frame(
            &mut stdin,
            &EngineRequest::Hello {
                version: PROTOCOL_VERSION,
            },
        ) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("engine handshake: {error}"));
        }
        let ready = read_frame::<_, EngineReply>(&mut stdout)
            .map_err(|error| format!("engine handshake: {error}"))?
            .ok_or("engine process stopped during handshake")?;
        if !matches!(ready, EngineReply::Ready { version } if version == PROTOCOL_VERSION) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("engine process rejected the protocol version".into());
        }
        write_frame(
            &mut stdin,
            &EngineRequest::OpenDevice {
                project_rate,
                settings: settings.clone(),
            },
        )
        .map_err(|error| format!("open engine device: {error}"))?;
        let opened = read_frame::<_, EngineReply>(&mut stdout)
            .map_err(|error| format!("open engine device: {error}"))?
            .ok_or("engine process stopped while opening the audio device")?;
        let snapshot = match opened {
            EngineReply::DeviceOpened {
                driver,
                device_name,
                sample_rate,
                channels,
                block_size,
            } => AudioSnapshot {
                running: true,
                driver,
                device_name,
                sample_rate,
                channels,
                block_size,
                ..AudioSnapshot::default()
            },
            EngineReply::Error(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            other => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("unexpected engine reply: {other:?}"));
            }
        };

        tracing::info!(
            driver = %snapshot.driver,
            device = %snapshot.device_name,
            sample_rate = snapshot.sample_rate,
            "engine process ready"
        );
        let snapshot = Arc::new(Mutex::new(snapshot));
        let (request_tx, request_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        std::thread::Builder::new()
            .name("notwig-engine-ipc-writer".into())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let shutdown = matches!(request, EngineRequest::Shutdown);
                    if write_frame(&mut stdin, &request).is_err() || shutdown {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start engine writer: {error}"))?;

        let reader_snapshot = Arc::clone(&snapshot);
        let reader_events = event_tx;
        std::thread::Builder::new()
            .name("notwig-engine-ipc-reader".into())
            .spawn(move || {
                loop {
                    let reply = match read_frame::<_, EngineReply>(&mut stdout) {
                        Ok(Some(reply)) => reply,
                        Ok(None) => break,
                        Err(error) => {
                            let _ = reader_events.send(EngineEvent::Error(format!(
                                "engine IPC read failed: {error}"
                            )));
                            break;
                        }
                    };
                    match reply {
                        EngineReply::Ready { .. } | EngineReply::DeviceOpened { .. } => {}
                        EngineReply::Snapshot(mut next) => {
                            let changed_playhead = reader_snapshot.lock().map_or(true, |current| {
                                current.playhead_samples != next.playhead_samples
                            });
                            if next.playhead_age_nanos > 0 {
                                next.playhead_updated = Some(
                                    Instant::now()
                                        .checked_sub(Duration::from_nanos(next.playhead_age_nanos))
                                        .unwrap_or_else(Instant::now),
                                );
                            } else if changed_playhead {
                                next.playhead_updated = Some(Instant::now());
                            }
                            next.playhead_age_nanos = 0;
                            if let Ok(mut snapshot) = reader_snapshot.lock() {
                                *snapshot = *next;
                            }
                        }
                        EngineReply::BuildFinished(result) => {
                            tracing::info!(?result, "engine process build finished");
                            let _ = reader_events.send(EngineEvent::BuildFinished(result));
                        }
                        EngineReply::EditorOpened { device_id } => {
                            let _ = reader_events.send(EngineEvent::EditorOpened { device_id });
                        }
                        EngineReply::EditorClosed {
                            device_id,
                            state,
                            error,
                        } => {
                            let _ = reader_events.send(EngineEvent::EditorClosed {
                                device_id,
                                state,
                                error,
                            });
                        }
                        EngineReply::EditorParam {
                            device_id,
                            param_id,
                            value,
                        } => {
                            let _ = reader_events.send(EngineEvent::EditorParam {
                                device_id,
                                param_id,
                                value,
                            });
                        }
                        EngineReply::EditorTransportToggle => {
                            let _ = reader_events.send(EngineEvent::EditorTransportToggle);
                        }
                        EngineReply::InputError(error) => {
                            let _ = reader_events.send(EngineEvent::InputError(error));
                        }
                        EngineReply::RecordingFinished(captures) => {
                            let _ = reader_events.send(EngineEvent::RecordingFinished(captures));
                        }
                        EngineReply::RecordingFailed(error) => {
                            let _ = reader_events.send(EngineEvent::RecordingFailed(error));
                        }
                        EngineReply::Error(error) => {
                            if let Ok(mut snapshot) = reader_snapshot.lock() {
                                snapshot.error = Some(error.clone());
                            }
                            let _ = reader_events.send(EngineEvent::Error(error));
                        }
                    }
                }
                tracing::warn!("engine process exited");
                if let Ok(mut snapshot) = reader_snapshot.lock() {
                    snapshot.running = false;
                    snapshot.crashed = true;
                }
                let _ = reader_events.send(EngineEvent::Error(
                    "Audio engine process exited unexpectedly.".into(),
                ));
            })
            .map_err(|error| format!("start engine reader: {error}"))?;

        Ok(Self {
            snapshot,
            requests: request_tx,
            events: event_rx,
            child,
        })
    }

    pub(crate) fn handle(&self) -> EngineClientHandle {
        EngineClientHandle {
            requests: self.requests.clone(),
        }
    }

    pub(crate) fn snapshot(&self) -> AudioSnapshot {
        self.snapshot
            .lock()
            .map_or_else(|_| AudioSnapshot::default(), |snapshot| snapshot.clone())
    }

    pub(crate) fn open_editor(
        &self,
        device_id: DeviceId,
        descriptor: PluginDescriptor,
        sample_rate: u32,
        state: Option<PluginState>,
        params: Vec<(ParamId, f64)>,
    ) -> Result<(), String> {
        self.requests
            .send(EngineRequest::OpenEditor {
                device_id,
                descriptor,
                sample_rate,
                state,
                params,
            })
            .map_err(|_| "audio engine process is not running".into())
    }

    pub(crate) fn close_editor(&self, device_id: DeviceId) {
        let _ = self.requests.send(EngineRequest::CloseEditor { device_id });
    }

    pub(crate) fn set_editor_param(&self, device_id: DeviceId, param_id: ParamId, value: f64) {
        let _ = self.requests.send(EngineRequest::SetEditorParam {
            device_id,
            param_id,
            value,
        });
    }

    pub(crate) fn poll(&self) -> Vec<EngineEvent> {
        self.events.try_iter().collect()
    }

    pub(crate) fn available_options() -> Vec<notwig_ui::AudioDriverView> {
        AudioService::available_options()
    }
}

impl Drop for EngineClient {
    fn drop(&mut self) {
        let _ = self.requests.send(EngineRequest::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
