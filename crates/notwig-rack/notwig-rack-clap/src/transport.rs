//! Host transport snapshot -> `clap_event_transport` conversion.

use notwig_rack_core::transport::TransportInfo;

use clap_sys::events::{
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_TRANSPORT, CLAP_TRANSPORT_HAS_BEATS_TIMELINE,
    CLAP_TRANSPORT_HAS_SECONDS_TIMELINE, CLAP_TRANSPORT_HAS_TEMPO,
    CLAP_TRANSPORT_HAS_TIME_SIGNATURE, CLAP_TRANSPORT_IS_LOOP_ACTIVE, CLAP_TRANSPORT_IS_PLAYING,
    CLAP_TRANSPORT_IS_RECORDING, clap_event_header, clap_event_transport, clap_transport_flags,
};
use clap_sys::fixedpoint::{CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR};

/// Translate a host [`TransportInfo`] snapshot into CLAP's
/// `clap_event_transport`. `sample_rate` converts the sample
/// position into the seconds timeline CLAP also wants.
///
/// CLAP beat / second times are 31.32 fixed-point (see
/// `CLAP_BEATTIME_FACTOR`); beats are quarter notes.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
pub(crate) fn build_clap_transport(t: &TransportInfo, sample_rate: f64) -> clap_event_transport {
    let beats_to_fixed = |b: f64| (b * CLAP_BEATTIME_FACTOR as f64).round() as i64;
    let secs_to_fixed = |s: f64| (s * CLAP_SECTIME_FACTOR as f64).round() as i64;

    let mut flags: clap_transport_flags = 0;
    let tempo = t.tempo_bpm.unwrap_or(0.0);
    if t.tempo_bpm.is_some() {
        flags |= CLAP_TRANSPORT_HAS_TEMPO;
    }
    let song_pos_beats = match t.song_position_beats {
        Some(b) => {
            flags |= CLAP_TRANSPORT_HAS_BEATS_TIMELINE;
            beats_to_fixed(b)
        }
        None => 0,
    };
    let song_pos_seconds = match t.song_position_samples {
        Some(s) => {
            flags |= CLAP_TRANSPORT_HAS_SECONDS_TIMELINE;
            secs_to_fixed(s as f64 / sample_rate.max(1.0))
        }
        None => 0,
    };
    let (tsig_num, tsig_denom) = match t.time_signature {
        Some((n, d)) => {
            flags |= CLAP_TRANSPORT_HAS_TIME_SIGNATURE;
            (n as u16, d as u16)
        }
        None => (0, 0),
    };
    let bar_start = t.bar_start_beats.map_or(0, beats_to_fixed);
    // Bar index: bar_start (in quarter notes) divided by the bar
    // length the time signature implies.
    let bar_number = match (t.bar_start_beats, t.time_signature) {
        (Some(bsb), Some((n, d))) => {
            let beats_per_bar = f64::from(n) * 4.0 / f64::from(d.max(1));
            (bsb / beats_per_bar.max(f64::EPSILON)).round() as i32
        }
        _ => 0,
    };
    if t.playing {
        flags |= CLAP_TRANSPORT_IS_PLAYING;
    }
    if t.recording {
        flags |= CLAP_TRANSPORT_IS_RECORDING;
    }
    if t.loop_active {
        flags |= CLAP_TRANSPORT_IS_LOOP_ACTIVE;
    }

    clap_event_transport {
        header: clap_event_header {
            size: u32::try_from(std::mem::size_of::<clap_event_transport>()).unwrap_or(0),
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_TRANSPORT,
            flags: 0,
        },
        flags,
        song_pos_beats,
        song_pos_seconds,
        tempo,
        tempo_inc: 0.0,
        loop_start_beats: 0,
        loop_end_beats: 0,
        loop_start_seconds: 0,
        loop_end_seconds: 0,
        bar_start,
        bar_number,
        tsig_num,
        tsig_denom,
    }
}
