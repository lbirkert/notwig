use super::*;

#[test]
fn layout_from_channel_counts() {
    let layout = layout_for(2, 2);
    assert_eq!(layout.total_input_channels(), 2);
    assert_eq!(layout.total_output_channels(), 2);
    let mono = layout_for(1, 1);
    assert_eq!(mono.total_input_channels(), 1);
    assert_eq!(mono.total_output_channels(), 1);
    let instrument = layout_for(0, 2);
    assert_eq!(instrument.total_input_channels(), 0);
    assert_eq!(instrument.total_output_channels(), 2);
}

#[test]
fn midi_body_decodes_channel_voice() {
    let note = midi_body([0x90, 60, 100, 0]);
    assert!(matches!(
        note,
        MidiData::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100
        }
    ));
    let off = midi_body([0x90, 60, 0, 0]);
    assert!(matches!(
        off,
        MidiData::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0
        }
    ));
    let bend = midi_body([0xE1, 0x00, 0x40, 0]);
    assert!(matches!(
        bend,
        MidiData::PitchBend {
            channel: 1,
            value: 8192
        }
    ));
}

#[test]
fn midi_to_bytes_round_trips_note() {
    let raw = midi_to_bytes(MidiData::NoteOn {
        channel: 3,
        note: 64,
        velocity: 127,
    })
    .expect("bytes");
    assert_eq!(raw.len, 3);
    assert_eq!(&raw.data[..3], &[0x93, 64, 127]);
}

#[test]
fn host_callback_receives_midi_events() {
    let shared = HostShared::new(None);
    let mut effect = AEffect {
        resvd1: (&raw const shared).cast_mut().cast(),
        ..AEffect::default()
    };

    let midi = MidiEvent {
        kind: abi::VST_MIDI_TYPE,
        byte_size: (std::mem::size_of::<MidiEvent>() - 8) as i32,
        delta_frames: 3,
        midi_data: [0x90, 60, 100, 0],
        ..MidiEvent::default()
    };
    let mut list = VstEventList {
        num_events: 1,
        events: [(&raw const midi).cast(); MAX_EVENTS],
        ..VstEventList::default()
    };
    let result = unsafe {
        host_callback(
            &raw mut effect,
            abi::HOST_PROCESS_EVENTS,
            0,
            0,
            (&raw mut list).cast(),
            0.0,
        )
    };
    assert_eq!(result, 1);
    let event = shared.output_events.pop().expect("event");
    assert_eq!(event.sample_offset, 3);
    assert!(matches!(
        event.body,
        EventBody::Midi(MidiData::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100
        })
    ));
}

#[test]
fn host_callback_records_editor_resize() {
    let shared = HostShared::new(None);
    shared.editor_open.store(true, Ordering::Release);
    let mut effect = AEffect {
        resvd1: (&raw const shared).cast_mut().cast(),
        ..AEffect::default()
    };
    let result = unsafe {
        host_callback(
            &raw mut effect,
            abi::HOST_SIZE_WINDOW,
            640,
            480,
            ptr::null_mut(),
            0.0,
        )
    };
    assert_eq!(result, 1);
    assert_eq!(shared.take_editor_size(), Some((640, 480)));
}

#[test]
fn time_state_reports_transport() {
    let state = TimeState::default();
    state.update(Some(TransportInfo {
        tempo_bpm: Some(128.0),
        time_signature: Some((7, 8)),
        song_position_beats: Some(16.0),
        song_position_samples: Some(48_000),
        bar_start_beats: Some(12.0),
        playing: true,
        recording: false,
        loop_active: true,
    }));
    let pointer = state.pointer(48_000.0) as *const TimeInfo;
    let time = unsafe { *pointer };
    assert!((time.tempo - 128.0).abs() < f64::EPSILON);
    assert!((time.ppq_position - 16.0).abs() < f64::EPSILON);
    assert_eq!(time.time_sig_numerator, 7);
    assert_eq!(time.time_sig_denominator, 8);
    assert_ne!(time.flags & abi::KVST_TRANSPORT_PLAYING, 0);
    assert_ne!(time.flags & abi::KVST_TRANSPORT_CYCLE_ACTIVE, 0);
}

#[test]
fn zero_editor_rect_is_rejected() {
    assert_eq!(
        rect_size(Rect {
            top: 0,
            left: 0,
            bottom: 0,
            right: 0,
        }),
        None
    );
    assert_eq!(
        rect_size(Rect {
            top: 0,
            left: 0,
            bottom: 480,
            right: 640,
        }),
        Some((640, 480))
    );
}

#[test]
fn default_paths_contains_a_vst_directory() {
    let paths = default_vst2_paths();
    assert!(paths.iter().any(|path| {
        path.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| {
                name.eq_ignore_ascii_case("vst")
                    || name.eq_ignore_ascii_case("vstplugins")
                    || name.eq_ignore_ascii_case("vst2")
            })
    }));
}
