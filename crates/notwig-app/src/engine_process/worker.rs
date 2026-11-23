use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use notwig_core::dsp::{
    CompositeDeviceFactory, DeviceFactory, MonitorRegistry, StockDeviceFactory,
};
use notwig_engine::{Engine, EngineControl, EngineControlUpdate, PluginDeviceFactory};
use notwig_ext::ExtensionHost;
use notwig_plugins::PluginHost;

use super::protocol::{EngineReply, WireBuild};
use crate::audio::{AudioService, AudioSettings};
use crate::recording::RecordingSession;

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(16);

pub(super) enum WorkerCommand {
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

type BuildReceiver = mpsc::Receiver<Result<Box<notwig_engine::PreparedRuntime>, String>>;

struct PendingBuild {
    generation: u64,
    started: Instant,
    receiver: BuildReceiver,
}

pub(super) struct EngineWorker {
    control: EngineControl,
    service: Option<AudioService>,
    monitors: Arc<MonitorRegistry>,
    replies: mpsc::Sender<EngineReply>,
    input_routes: Vec<(notwig_core::TrackId, u16)>,
    recording: Option<RecordingSession>,
    build_receiver: Option<PendingBuild>,
    queued_build: Option<WireBuild>,
    build_generation: u64,
}

impl EngineWorker {
    pub(super) fn new(replies: mpsc::Sender<EngineReply>, monitors: Arc<MonitorRegistry>) -> Self {
        Self {
            control: EngineControl::new(),
            service: None,
            monitors,
            replies,
            input_routes: Vec::new(),
            recording: None,
            build_receiver: None,
            queued_build: None,
            build_generation: 0,
        }
    }

    pub(super) fn run(mut self, receiver: mpsc::Receiver<WorkerCommand>) {
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
            self.poll_build();
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
        self.build_generation = self.build_generation.wrapping_add(1);
        self.build_receiver = None;
        self.queued_build = None;
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
        if self.build_receiver.is_some() {
            self.queued_build = Some(build);
            return;
        }
        self.start_build(build);
    }

    fn start_build(&mut self, build: WireBuild) {
        self.build_generation = self.build_generation.wrapping_add(1);
        let generation = self.build_generation;
        tracing::info!("audio engine host building runtime");
        let (sender, receiver) = mpsc::channel();
        let monitors = Arc::clone(&self.monitors);
        let spawned = std::thread::Builder::new()
            .name("notwig-engine-loader".into())
            .spawn(move || {
                let result = build_runtime(build, monitors);
                let _ = sender.send(result);
            });
        if let Err(error) = spawned {
            let _ = self.replies.send(EngineReply::BuildFinished(Err(format!(
                "start engine loader: {error}"
            ))));
            return;
        }
        self.build_receiver = Some(PendingBuild {
            generation,
            started: Instant::now(),
            receiver,
        });
    }

    fn poll_build(&mut self) {
        let Some(pending) = &self.build_receiver else {
            if let Some(build) = self.queued_build.take() {
                self.start_build(build);
            }
            return;
        };
        match pending.receiver.try_recv() {
            Ok(result) => {
                let pending = self.build_receiver.take().expect("build was just checked");
                let elapsed_ms = pending.started.elapsed().as_millis();
                let current =
                    pending.generation == self.build_generation && self.queued_build.is_none();
                if current {
                    match result {
                        Ok(runtime) => {
                            if self.service.is_some() {
                                self.control.data_mut().set_prepared_runtime(runtime);
                                self.control.flush();
                                tracing::info!(
                                    generation = pending.generation,
                                    elapsed_ms,
                                    "Engine runtime installed"
                                );
                                let _ = self.replies.send(EngineReply::BuildFinished(Ok(())));
                            } else {
                                let _ = self.replies.send(EngineReply::BuildFinished(Err(
                                    "audio device is not open".into(),
                                )));
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                generation = pending.generation,
                                elapsed_ms,
                                error = %error,
                                "Engine build failed"
                            );
                            let _ = self.replies.send(EngineReply::BuildFinished(Err(error)));
                        }
                    }
                } else {
                    tracing::debug!(
                        generation = pending.generation,
                        elapsed_ms,
                        "Discarding superseded engine build"
                    );
                }
                if let Some(build) = self.queued_build.take() {
                    self.start_build(build);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.build_receiver = None;
                let _ = self.replies.send(EngineReply::BuildFinished(Err(
                    "engine loader stopped unexpectedly".into(),
                )));
                if let Some(build) = self.queued_build.take() {
                    self.start_build(build);
                }
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
        for bypass in update.device_bypasses {
            data.push_device_bypass(bypass);
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
        if let Some(enabled) = update.low_latency_monitoring {
            data.set_low_latency_monitoring(enabled);
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
