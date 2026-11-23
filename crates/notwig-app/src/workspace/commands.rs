use notwig_core::apply;
use notwig_core::protocol::{Command, CommandPayload, EngineMsg};
use notwig_engine::TransportUpdate;

use super::Workspace;

impl Workspace {
    pub fn apply(&mut self, payload: CommandPayload) -> Result<(), String> {
        let preserve_playhead = matches!(
            &payload,
            CommandPayload::SetTransport {
                playing: None,
                playhead: None,
                ..
            }
        );
        let metronome_only = matches!(
            &payload,
            CommandPayload::SetTransport {
                playing: None,
                playhead: None,
                loop_enabled: None,
                loop_range: None,
                metronome: Some(_),
            }
        );
        let structural = payload.requires_plan_rebuild();
        let record_undo = !matches!(
            payload,
            CommandPayload::SetTransport { .. }
                | CommandPayload::SetRecording { .. }
                | CommandPayload::Pause
                | CommandPayload::Stop
        );
        let before = record_undo.then(|| self.doc.clone());
        match apply(&self.doc, &Command::new(payload)) {
            Ok(result) => {
                self.doc = result.doc;
                self.dirty = true;
                if let Some(before) = before {
                    self.history.record(before, structural);
                }
                if structural {
                    self.request_engine_rebuild();
                } else if metronome_only {
                    self.publish_metronome();
                } else {
                    let (mut transport, all_notes_off) = control_updates(result.engine_msgs);
                    if preserve_playhead && let Some(update) = transport.as_mut() {
                        update.playhead_samples = self.preserved_playhead_samples();
                    }
                    self.publish_engine_control(transport, all_notes_off, None);
                }
                self.error = None;
                Ok(())
            }
            Err(err) => {
                self.error = Some(err.to_string());
                Err(err.to_string())
            }
        }
    }

    pub fn undo(&mut self) -> Result<(), String> {
        let Some(entry) = self.history.undo(&self.doc) else {
            return Ok(());
        };
        self.doc = entry.doc;
        self.dirty = true;
        if entry.structural {
            self.request_engine_rebuild();
        } else {
            self.sync_engine();
        }
        self.status = Some("Undo.".into());
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        let Some(entry) = self.history.redo(&self.doc) else {
            return Ok(());
        };
        self.doc = entry.doc;
        self.dirty = true;
        if entry.structural {
            self.request_engine_rebuild();
        } else {
            self.sync_engine();
        }
        self.status = Some("Redo.".into());
        Ok(())
    }

    pub fn apply_batch(&mut self, payloads: Vec<CommandPayload>) -> Result<(), String> {
        if payloads.is_empty() {
            return Ok(());
        }
        let before = self.doc.clone();
        let mut doc = self.doc.clone();
        let mut structural = false;
        let mut engine_msgs = Vec::new();
        for payload in payloads {
            structural |= payload.requires_plan_rebuild();
            let result = apply(&doc, &Command::new(payload)).map_err(|err| err.to_string())?;
            doc = result.doc;
            engine_msgs.extend(result.engine_msgs);
        }
        self.doc = doc;
        self.dirty = true;
        self.history.record(before, structural);
        if structural {
            self.request_engine_rebuild();
        } else {
            let (transport, all_notes_off) = control_updates(engine_msgs);
            self.publish_engine_control(transport, all_notes_off, None);
        }
        self.error = None;
        Ok(())
    }
}

fn control_updates(messages: Vec<EngineMsg>) -> (Option<TransportUpdate>, bool) {
    let mut transport = None;
    let mut all_notes_off = false;
    for message in messages {
        match message {
            EngineMsg::SetTransport {
                playing,
                playhead_samples,
                loop_enabled,
                loop_start_samples,
                loop_end_samples,
                stop,
                metronome,
            } => {
                transport = Some(TransportUpdate {
                    playing,
                    playhead_samples,
                    loop_enabled,
                    loop_start_samples,
                    loop_end_samples,
                    stop,
                    metronome,
                });
            }
            EngineMsg::AllNotesOff => all_notes_off = true,
            EngineMsg::ApplyPlan { .. }
            | EngineMsg::SetParam { .. }
            | EngineMsg::SetTrackState { .. }
            | EngineMsg::UpdateSchedule { .. }
            | EngineMsg::PrepareAudioAsset { .. } => {}
        }
    }
    (transport, all_notes_off)
}
