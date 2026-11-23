use crate::CoreError;
use crate::domain::ProjectDoc;
use crate::protocol::{DocChange, EngineMsg};
use crate::time::{RangeMus, TimeMus};

use super::audio::rescale_unsliced_audio_clips;
use super::dispatch::{Changes, schedule_msg};

pub(super) fn set_transport(
    doc: &mut ProjectDoc,
    playing: Option<bool>,
    playhead: Option<TimeMus>,
    loop_enabled: Option<bool>,
    loop_range: Option<Option<RangeMus>>,
    metronome: Option<bool>,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    if let Some(value) = playing {
        doc.transport.playing = value;
    }
    if let Some(value) = playhead {
        if value < TimeMus::zero() {
            return Err(CoreError::ValidationFailed {
                reason: "playhead must not be negative".into(),
            });
        }
        doc.transport.playhead = value;
    }
    if let Some(value) = loop_enabled {
        doc.transport.loop_enabled = value;
    }
    if let Some(value) = loop_range {
        doc.transport.loop_range = value;
    }
    if let Some(value) = metronome {
        doc.transport.metronome_enabled = value;
    }
    doc.transport.validate()?;
    changes.push(DocChange::Transport(doc.transport.clone()));
    Ok(vec![transport_msg(doc)])
}

pub(super) fn set_tempo(
    doc: &mut ProjectDoc,
    tempo_bpm: f64,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    let previous_bpm = doc.meta.tempo_bpm;
    doc.meta.tempo_bpm = tempo_bpm;
    if (previous_bpm - tempo_bpm).abs() > f64::EPSILON {
        rescale_unsliced_audio_clips(doc, changes)?;
    }
    doc.validate()?;
    changes.push(DocChange::Meta(doc.meta.clone()));
    Ok(vec![schedule_msg(doc), transport_msg(doc)])
}

pub(super) fn set_time_signature(
    doc: &mut ProjectDoc,
    numerator: u8,
    denominator: u8,
    changes: &mut Changes,
) -> Result<Vec<EngineMsg>, CoreError> {
    doc.meta.time_sig_num = numerator;
    doc.meta.time_sig_den = denominator;
    doc.validate()?;
    changes.push(DocChange::Meta(doc.meta.clone()));
    Ok(Vec::new())
}

pub(super) fn pause(doc: &mut ProjectDoc, changes: &mut Changes) -> Vec<EngineMsg> {
    doc.transport.playing = false;
    doc.transport.recording = false;
    changes.push(DocChange::Transport(doc.transport.clone()));
    vec![transport_msg(doc)]
}

pub(super) fn stop(doc: &mut ProjectDoc, changes: &mut Changes) -> Vec<EngineMsg> {
    doc.transport.playing = false;
    doc.transport.recording = false;
    doc.transport.playhead = match doc.transport.loop_range {
        Some(range) if doc.transport.loop_enabled => range.start,
        _ => TimeMus::zero(),
    };
    changes.push(DocChange::Transport(doc.transport.clone()));
    vec![transport_msg(doc), EngineMsg::AllNotesOff]
}

pub(super) fn set_recording(
    doc: &mut ProjectDoc,
    recording: bool,
    changes: &mut Changes,
) -> Vec<EngineMsg> {
    doc.transport.recording = recording;
    changes.push(DocChange::Transport(doc.transport.clone()));
    Vec::new()
}

fn transport_msg(doc: &ProjectDoc) -> EngineMsg {
    let map = doc.tempo_map();
    let playhead_samples = map.mus_to_samples(doc.transport.playhead);
    let (start, end) = doc.transport.loop_range.map_or((None, None), |range| {
        (
            Some(map.mus_to_samples(range.start)),
            Some(map.mus_to_samples(range.end)),
        )
    });
    EngineMsg::SetTransport {
        playing: doc.transport.playing,
        playhead_samples,
        loop_enabled: doc.transport.loop_enabled,
        loop_start_samples: start,
        loop_end_samples: end,
        stop: false,
        metronome: Some(crate::protocol::MetronomeState {
            enabled: doc.transport.metronome_enabled,
            beats_per_bar: doc.meta.time_sig_num,
            beat_length: doc.meta.beat_length(),
            bar_length: doc.meta.bar_length(),
        }),
    }
}
