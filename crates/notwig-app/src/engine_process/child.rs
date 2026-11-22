use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use notwig_core::dsp::{
    CompositeDeviceFactory, DeviceFactory, MonitorRegistry, StockDeviceFactory,
};
use notwig_core::{DeviceId, ParamId};
use notwig_engine::{Engine, EngineControl, EngineControlUpdate, PluginDeviceFactory};
use notwig_ext::ExtensionHost;
use notwig_plugins::{NativePluginWindow, PluginHost, prepare_window_host, pump_window_host};

use super::protocol::{
    EngineReply, EngineRequest, PROTOCOL_VERSION, WireBuild, read_frame, write_frame,
};
use crate::audio::{AudioService, AudioSettings};
use crate::recording::RecordingSession;

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(16);

pub(crate) fn run_engine_host() -> Result<(), String> {
    tracing::info!("audio engine host starting");
    let running = Arc::new(AtomicBool::new(true));
    let (worker_tx, worker_rx) = mpsc::channel();
    let (editor_tx, editor_rx) = mpsc::channel();
    let (reply_tx, reply_rx) = mpsc::channel();
    let monitors = Arc::new(MonitorRegistry::default());

    let _writer = std::thread::Builder::new()
        .name("notwig-engine-ipc-writer".into())
        .spawn(move || {
            let mut stdout = io::stdout().lock();
            while let Ok(reply) = reply_rx.recv() {
                if write_frame(&mut stdout, &reply).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| format!("start engine IPC writer: {error}"))?;

    let worker_replies = reply_tx.clone();
    let worker_monitors = Arc::clone(&monitors);
    let _worker = std::thread::Builder::new()
        .name("notwig-engine-worker".into())
        .spawn(move || {
            EngineWorker::new(worker_replies, worker_monitors).run(worker_rx);
        })
        .map_err(|error| format!("start engine worker: {error}"))?;

    let ipc_running = Arc::clone(&running);
    let ipc_replies = reply_tx.clone();
    let _reader = std::thread::Builder::new()
        .name("notwig-engine-ipc-reader".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            loop {
                let request = match read_frame::<_, EngineRequest>(&mut stdin) {
                    Ok(Some(request)) => request,
                    Ok(None) => break,
                    Err(error) => {
                        let _ = ipc_replies.send(EngineReply::Error(format!(
                            "engine IPC read failed: {error}"
                        )));
                        break;
                    }
                };
                match request {
                    EngineRequest::Hello { version } => {
                        if version != PROTOCOL_VERSION {
                            let _ = ipc_replies.send(EngineReply::Error(format!(
                                "engine protocol {version} does not match {PROTOCOL_VERSION}"
                            )));
                            break;
                        }
                        let _ = ipc_replies.send(EngineReply::Ready {
                            version: PROTOCOL_VERSION,
                        });
                    }
                    EngineRequest::OpenDevice {
                        project_rate,
                        settings,
                    } => {
                        if worker_tx
                            .send(WorkerCommand::OpenDevice {
                                project_rate,
                                settings,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    EngineRequest::Build(build) => {
                        if worker_tx.send(WorkerCommand::Build(build)).is_err() {
                            break;
                        }
                    }
                    EngineRequest::Control(update) => {
                        if worker_tx.send(WorkerCommand::Control(update)).is_err() {
                            break;
                        }
                    }
                    EngineRequest::ConfigureInputs { requests } => {
                        if worker_tx
                            .send(WorkerCommand::ConfigureInputs { requests })
                            .is_err()
                        {
                            break;
                        }
                    }
                    EngineRequest::StartRecording => {
                        if worker_tx.send(WorkerCommand::StartRecording).is_err() {
                            break;
                        }
                    }
                    EngineRequest::StopRecording => {
                        if worker_tx.send(WorkerCommand::StopRecording).is_err() {
                            break;
                        }
                    }
                    EngineRequest::OpenEditor {
                        device_id,
                        descriptor,
                        sample_rate,
                        state,
                        params,
                    } => {
                        let _ = editor_tx.send(EditorCommand::Open {
                            device_id,
                            descriptor,
                            sample_rate,
                            state,
                            params,
                        });
                    }
                    EngineRequest::CloseEditor { device_id } => {
                        let _ = editor_tx.send(EditorCommand::Close { device_id });
                    }
                    EngineRequest::FocusEditor { device_id } => {
                        let _ = editor_tx.send(EditorCommand::Focus { device_id });
                    }
                    EngineRequest::SetEditorParam {
                        device_id,
                        param_id,
                        value,
                    } => {
                        let _ = editor_tx.send(EditorCommand::SetParam {
                            device_id,
                            param_id,
                            value,
                        });
                    }
                    EngineRequest::Shutdown => break,
                }
            }
            ipc_running.store(false, Ordering::SeqCst);
            let _ = worker_tx.send(WorkerCommand::Shutdown);
            let _ = editor_tx.send(EditorCommand::Shutdown);
        })
        .map_err(|error| format!("start engine IPC reader: {error}"))?;

    prepare_window_host();
    let mut editors = EditorHost::new(reply_tx.clone(), monitors);
    while running.load(Ordering::Relaxed) {
        while let Ok(command) = editor_rx.try_recv() {
            match command {
                EditorCommand::Open {
                    device_id,
                    descriptor,
                    sample_rate,
                    state,
                    params,
                } => editors.open(device_id, descriptor, sample_rate, state, params),
                EditorCommand::Close { device_id } => editors.close(device_id),
                EditorCommand::Focus { device_id } => editors.focus(device_id),
                EditorCommand::SetParam {
                    device_id,
                    param_id,
                    value,
                } => editors.set_param(device_id, param_id, value),
                EditorCommand::Shutdown => {
                    editors.close_all();
                    running.store(false, Ordering::SeqCst);
                }
            }
        }
        editors.pump();
        pump_window_host();
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

enum WorkerCommand {
    OpenDevice {
        project_rate: u32,
        settings: AudioSettings,
    },
    Build(WireBuild),
    Control(EngineControlUpdate),
    ConfigureInputs {
        requests: Vec<(notwig_core::TrackId, notwig_core::AudioInputRef)>,
    },
    StartRecording,
    StopRecording,
    Shutdown,
}

struct EngineWorker {
    control: EngineControl,
    service: Option<AudioService>,
    monitors: Arc<MonitorRegistry>,
    replies: mpsc::Sender<EngineReply>,
    input_routes: Vec<(notwig_core::TrackId, u16)>,
    recording: Option<RecordingSession>,
}

impl EngineWorker {
    fn new(replies: mpsc::Sender<EngineReply>, monitors: Arc<MonitorRegistry>) -> Self {
        Self {
            control: EngineControl::new(),
            service: None,
            monitors,
            replies,
            input_routes: Vec::new(),
            recording: None,
        }
    }

    fn run(mut self, receiver: mpsc::Receiver<WorkerCommand>) {
        loop {
            match receiver.recv_timeout(SNAPSHOT_INTERVAL) {
                Ok(WorkerCommand::OpenDevice {
                    project_rate,
                    settings,
                }) => self.open_device(project_rate, &settings),
                Ok(WorkerCommand::Build(build)) => self.build(build),
                Ok(WorkerCommand::Control(update)) => self.apply_control(update),
                Ok(WorkerCommand::ConfigureInputs { requests }) => self.configure_inputs(requests),
                Ok(WorkerCommand::StartRecording) => self.start_recording(),
                Ok(WorkerCommand::StopRecording) => self.stop_recording(),
                Ok(WorkerCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.stop_recording_silent();
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(service) = &self.service {
                let mut snapshot = service.snapshot();
                if let Some(recording) = &self.recording {
                    let captures = recording.take_captured();
                    if !captures.is_empty() {
                        let _ = self.replies.send(EngineReply::RecordingChunk {
                            sample_rate: snapshot.sample_rate,
                            captures: captures.into_iter().collect(),
                        });
                    }
                }
                if let Some(updated) = snapshot.playhead_updated {
                    snapshot.playhead_age_nanos =
                        u64::try_from(updated.elapsed().as_nanos()).unwrap_or(u64::MAX);
                }
                snapshot.playhead_updated = None;
                let _ = self.replies.send(EngineReply::Snapshot(Box::new(snapshot)));
            }
        }
        self.service = None;
    }

    fn open_device(&mut self, project_rate: u32, settings: &AudioSettings) {
        self.stop_recording_silent();
        self.service = None;
        self.input_routes.clear();
        self.control.data_mut().clear();
        match AudioService::start_with(self.control.handle(), project_rate, settings) {
            Ok(service) => {
                let snapshot = service.snapshot();
                tracing::info!(
                    driver = %snapshot.driver,
                    device = %snapshot.device_name,
                    sample_rate = snapshot.sample_rate,
                    channels = snapshot.channels,
                    "audio engine device opened"
                );
                let _ = self.replies.send(EngineReply::DeviceOpened {
                    driver: snapshot.driver,
                    device_name: snapshot.device_name,
                    sample_rate: snapshot.sample_rate,
                    channels: snapshot.channels,
                    block_size: snapshot.block_size,
                });
                self.service = Some(service);
            }
            Err(error) => {
                let _ = self.replies.send(EngineReply::Error(error));
            }
        }
    }

    fn build(&mut self, build: WireBuild) {
        if self.service.is_none() {
            let _ = self.replies.send(EngineReply::BuildFinished(Err(
                "audio device is not open".into()
            )));
            return;
        }
        tracing::info!("audio engine host building runtime");
        let result = build_runtime(build, Arc::clone(&self.monitors));
        match result {
            Ok(runtime) => {
                self.control.data_mut().set_prepared_runtime(runtime);
                self.control.flush();
                tracing::info!("audio engine host runtime installed");
                let _ = self.replies.send(EngineReply::BuildFinished(Ok(())));
            }
            Err(error) => {
                tracing::warn!(error = %error, "audio engine host build failed");
                let _ = self.replies.send(EngineReply::BuildFinished(Err(error)));
            }
        }
    }

    fn configure_inputs(
        &mut self,
        requests: Vec<(notwig_core::TrackId, notwig_core::AudioInputRef)>,
    ) {
        let Some(service) = &self.service else {
            let _ = self
                .replies
                .send(EngineReply::InputError("audio device is not open".into()));
            return;
        };
        match service.configure_inputs(&requests) {
            Ok(routes) => {
                self.input_routes = routes;
                let routes = self
                    .input_routes
                    .iter()
                    .map(|(track_id, slot)| notwig_engine::InputRoute {
                        track_id: *track_id,
                        slot: *slot,
                    })
                    .collect();
                self.control.data_mut().set_input_routes(routes);
                self.control.flush();
            }
            Err(error) => {
                self.input_routes.clear();
                self.control.data_mut().set_input_routes(Vec::new());
                self.control.flush();
                let _ = self.replies.send(EngineReply::InputError(error));
            }
        }
    }

    fn start_recording(&mut self) {
        let Some(service) = &self.service else {
            let _ = self.replies.send(EngineReply::RecordingFailed(
                "audio device is not open".into(),
            ));
            return;
        };
        if self.recording.is_some() {
            return;
        }
        let sample_rate = service.snapshot().sample_rate;
        self.recording = Some(service.begin_recording(self.input_routes.clone(), sample_rate));
    }

    fn stop_recording_silent(&mut self) {
        if let Some(session) = self.recording.take() {
            let _ = session.finish();
        }
    }

    fn stop_recording(&mut self) {
        let Some(session) = self.recording.take() else {
            let _ = self
                .replies
                .send(EngineReply::RecordingFinished(Vec::new()));
            return;
        };
        match session.finish() {
            Ok(captures) => {
                let _ = self.replies.send(EngineReply::RecordingFinished(
                    captures.into_iter().collect(),
                ));
            }
            Err(error) => {
                let _ = self.replies.send(EngineReply::RecordingFailed(error));
            }
        }
    }

    fn apply_control(&mut self, update: EngineControlUpdate) {
        let data = self.control.data_mut();
        if let Some(schedule) = update.schedule {
            data.set_schedule(schedule);
        }
        if let Some(assets) = update.assets {
            data.set_assets(assets);
        }
        for track in update.track_states {
            data.push_track_state(track);
        }
        for param in update.params {
            data.push_param(param);
        }
        for preview in update.preview_notes {
            data.push_preview_note(preview);
        }
        for (track_id, note_id) in update.preview_offs {
            data.push_preview_off(track_id, note_id);
        }
        if let Some(transport) = update.transport {
            data.set_transport(transport);
        }
        if let Some(metronome) = update.metronome {
            data.set_metronome(metronome);
        }
        if update.all_notes_off {
            data.request_all_notes_off();
        }
        self.control.flush();
    }
}

fn build_runtime(
    build: WireBuild,
    monitors: Arc<MonitorRegistry>,
) -> Result<Box<notwig_engine::PreparedRuntime>, String> {
    let mut factories: Vec<Arc<dyn DeviceFactory>> = vec![Arc::new(StockDeviceFactory)];
    if let Some(root) = build.extension_root {
        let host = ExtensionHost::load(&root, &build.extension_settings)
            .map_err(|error| format!("load extensions: {error}"))?;
        factories.push(host.factory());
    }
    let plugin_factory: Arc<dyn DeviceFactory> = Arc::new(PluginDeviceFactory::with_monitors(
        PluginHost::new(),
        build.plugin_descriptors,
        build.plugin_states,
        monitors,
    ));
    factories.push(plugin_factory);
    let factory: Arc<dyn DeviceFactory> = Arc::new(CompositeDeviceFactory::new(factories));
    Engine::prepare_runtime(
        build.config,
        factory,
        Arc::new(build.plan),
        Arc::new(build.schedule),
        Arc::new(build.assets),
    )
    .map_err(|error| error.to_string())
}

enum EditorCommand {
    Open {
        device_id: DeviceId,
        descriptor: notwig_plugins::PluginDescriptor,
        sample_rate: u32,
        state: Option<notwig_plugins::PluginState>,
        params: Vec<(ParamId, f64)>,
    },
    Close {
        device_id: DeviceId,
    },
    Focus {
        device_id: DeviceId,
    },
    SetParam {
        device_id: DeviceId,
        param_id: ParamId,
        value: f64,
    },
    Shutdown,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(clippy::zero_sized_map_values, clippy::drop_non_drop)
)]
struct EditorHost {
    monitors: Arc<MonitorRegistry>,
    windows: BTreeMap<DeviceId, NativePluginWindow>,
    replies: mpsc::Sender<EngineReply>,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(clippy::zero_sized_map_values, clippy::drop_non_drop)
)]
impl EditorHost {
    fn new(replies: mpsc::Sender<EngineReply>, monitors: Arc<MonitorRegistry>) -> Self {
        Self {
            monitors,
            windows: BTreeMap::new(),
            replies,
        }
    }

    fn open(
        &mut self,
        device_id: DeviceId,
        descriptor: notwig_plugins::PluginDescriptor,
        sample_rate: u32,
        state: Option<notwig_plugins::PluginState>,
        params: Vec<(ParamId, f64)>,
    ) {
        if let Some(window) = self.windows.get(&device_id) {
            window.focus();
            let _ = self.replies.send(EngineReply::EditorOpened { device_id });
            return;
        }
        let monitor = self.monitors.monitor(device_id);
        let result = NativePluginWindow::open(&descriptor, sample_rate, Some(monitor));
        match result {
            Ok(mut window) => {
                if let Some(state) = state
                    && let Err(error) = window.load_state(&state)
                {
                    let _ = self.replies.send(EngineReply::EditorClosed {
                        device_id,
                        state: None,
                        error: Some(error.to_string()),
                    });
                    return;
                }
                for (param_id, value) in params {
                    window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
                }
                self.windows.insert(device_id, window);
                let _ = self.replies.send(EngineReply::EditorOpened { device_id });
            }
            Err(error) => {
                let _ = self.replies.send(EngineReply::EditorClosed {
                    device_id,
                    state: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    fn close(&mut self, device_id: DeviceId) {
        let Some(mut window) = self.windows.remove(&device_id) else {
            return;
        };
        let state = window.save_state().ok();
        drop(window);
        let _ = self.replies.send(EngineReply::EditorClosed {
            device_id,
            state,
            error: None,
        });
    }

    fn focus(&self, device_id: DeviceId) {
        if let Some(window) = self.windows.get(&device_id) {
            window.focus();
        }
    }

    fn set_param(&mut self, device_id: DeviceId, param_id: ParamId, value: f64) {
        if let Some(window) = self.windows.get_mut(&device_id) {
            window.set_parameter(param_id.into_uuid().as_u128() as u32, value);
        }
    }

    fn pump(&mut self) {
        let mut closed = Vec::new();
        let mut changes = Vec::new();
        let mut playback_toggle = false;
        for (device_id, window) in &mut self.windows {
            window.pump();
            playback_toggle |= window.take_playback_toggle();
            if !window.is_open() {
                closed.push(*device_id);
                continue;
            }
            for (raw_id, value) in window.poll_changes() {
                changes.push((*device_id, raw_id, value));
            }
        }
        if playback_toggle {
            let _ = self.replies.send(EngineReply::EditorTransportToggle);
        }
        for device_id in closed {
            self.close(device_id);
        }
        for (device_id, raw_id, value) in changes {
            let param_id = ParamId::from_uuid(uuid::Uuid::from_u128(u128::from(raw_id)));
            let _ = self.replies.send(EngineReply::EditorParam {
                device_id,
                param_id,
                value,
            });
        }
    }

    fn close_all(&mut self) {
        let device_ids: Vec<_> = self.windows.keys().copied().collect();
        for device_id in device_ids {
            self.close(device_id);
        }
    }
}
