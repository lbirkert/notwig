use notwig_core::protocol::CommandPayload;
use notwig_core::{ClipId, NoteId, Rational, TrackId};
use notwig_engine::{InputRoute, PreviewNote};

use super::Workspace;

impl Workspace {
    pub fn set_playhead(&mut self, playhead: notwig_core::TimeMus) -> Result<(), String> {
        if self.doc.transport.playhead == playhead {
            return Ok(());
        }
        self.apply(CommandPayload::SetTransport {
            playing: None,
            playhead: Some(playhead),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    /// Record the playhead for a save without seeking the audio engine.
    ///
    /// While the transport runs, the engine owns the live playhead. Publishing
    /// the interpolated UI playhead back as a transport update would rewind to a
    /// slightly stale sample and trigger an all-notes-off. Update the document
    /// directly instead; an actual seek goes through [`Self::set_playhead`].
    pub fn sync_playhead_for_save(&mut self, playhead: notwig_core::TimeMus) -> Result<(), String> {
        if self.doc.transport.playhead == playhead {
            return Ok(());
        }
        if self.doc.transport.playing {
            self.doc.transport.playhead = playhead;
            self.dirty = true;
            Ok(())
        } else {
            self.set_playhead(playhead)
        }
    }

    /// Stage a short, UI-triggered note preview for the audio engine.
    pub fn preview_note(
        &mut self,
        track_id: TrackId,
        note_id: NoteId,
        pitch: u8,
        velocity: f32,
        length_mus: Option<notwig_core::TimeMus>,
    ) {
        self.engine_control
            .data_mut()
            .push_preview_note(PreviewNote {
                track_id,
                note_id,
                pitch,
                velocity,
                length_mus,
            });
        self.engine_control.flush();
    }

    pub fn release_preview_note(&mut self, track_id: TrackId, note_id: NoteId) {
        self.engine_control
            .data_mut()
            .push_preview_off(track_id, note_id);
        self.engine_control.flush();
    }

    pub fn finish_recording(
        &mut self,
        track_id: TrackId,
        start: notwig_core::TimeMus,
        buffer: notwig_core::AudioAssetBuffer,
    ) -> Result<ClipId, String> {
        self.finish_audio_recording(track_id, start, buffer)
    }

    pub fn set_input_routes(&mut self, routes: Vec<(TrackId, u16)>) {
        self.engine_control.data_mut().set_input_routes(
            routes
                .into_iter()
                .map(|(track_id, slot)| InputRoute { track_id, slot })
                .collect(),
        );
        self.engine_control.flush();
    }

    pub fn play(&mut self) -> Result<(), String> {
        if !self.doc.transport.playing {
            self.play_start = Some(self.doc.transport.playhead);
        }
        self.apply(CommandPayload::SetTransport {
            playing: Some(true),
            playhead: Some(self.doc.transport.playhead),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.apply(CommandPayload::Pause)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        let start = self
            .play_start
            .take()
            .unwrap_or(self.doc.transport.playhead);
        self.apply(CommandPayload::SetTransport {
            playing: Some(false),
            playhead: Some(start),
            loop_enabled: None,
            loop_range: None,
            metronome: None,
        })
    }

    pub fn toggle_loop(&mut self) -> Result<(), String> {
        let enabled = !self.doc.transport.loop_enabled;
        let range = if enabled && self.doc.transport.loop_range.is_none() {
            Some(
                notwig_core::RangeMus::new(notwig_core::TimeMus::zero(), Rational::new(4, 1))
                    .map_err(|err| err.to_string())?,
            )
        } else {
            self.doc.transport.loop_range
        };
        self.apply(CommandPayload::SetTransport {
            playing: None,
            playhead: None,
            loop_enabled: Some(enabled),
            loop_range: Some(range),
            metronome: None,
        })
    }
}
