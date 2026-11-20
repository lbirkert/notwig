use notwig_ext_api::{NOTE_EVENT_BYTES, NoteEvent, NoteKind, decode_note_event, encode_note_event};

#[test]
fn note_events_round_trip_through_the_wire_format() {
    let event = NoteEvent {
        sample_offset: 123,
        kind: NoteKind::On,
        pitch: 69,
        velocity: 0.75,
    };
    let bytes = encode_note_event(&event);
    assert_eq!(bytes.len(), NOTE_EVENT_BYTES);
    assert_eq!(decode_note_event(&bytes), Some(event));
}

#[test]
fn invalid_note_events_are_rejected() {
    assert_eq!(decode_note_event(&[0; 4]), None);
    let mut bytes = encode_note_event(&NoteEvent {
        sample_offset: 0,
        kind: NoteKind::Off,
        pitch: 60,
        velocity: 1.0,
    });
    bytes[4] = 9;
    assert_eq!(decode_note_event(&bytes), None);
}
