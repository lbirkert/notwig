use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use notwig_core::dsp::MonitorRegistry;
use notwig_plugins::{prepare_window_host, pump_window_host};

use super::editor::{EditorCommand, EditorHost};
use super::protocol::{EngineReply, EngineRequest, PROTOCOL_VERSION, read_frame, write_frame};
use super::worker::{EngineWorker, WorkerCommand};

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
