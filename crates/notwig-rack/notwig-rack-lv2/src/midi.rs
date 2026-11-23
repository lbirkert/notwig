use notwig_rack_core::events::{EventBody, MidiData};

/// Encode one notwig-rack MIDI event into the (status, d1, d2, len)
/// triple LV2 atom MIDI events expect. Returns `None` for
/// non-MIDI events or unsupported MIDI variants.
pub(crate) fn midi_bytes(body: &EventBody) -> Option<(u8, u8, u8, usize)> {
    match body {
        EventBody::Midi(MidiData::NoteOn {
            channel,
            note,
            velocity,
        }) => Some((0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F, 3)),
        EventBody::Midi(MidiData::NoteOff {
            channel,
            note,
            velocity,
        }) => Some((0x80 | (channel & 0x0F), note & 0x7F, velocity & 0x7F, 3)),
        EventBody::Midi(MidiData::PolyAftertouch {
            channel,
            note,
            pressure,
        }) => Some((0xA0 | (channel & 0x0F), note & 0x7F, pressure & 0x7F, 3)),
        EventBody::Midi(MidiData::ControlChange {
            channel,
            controller,
            value,
        }) => Some((0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F, 3)),
        EventBody::Midi(MidiData::ProgramChange { channel, program }) => {
            Some((0xC0 | (channel & 0x0F), program & 0x7F, 0, 2))
        }
        EventBody::Midi(MidiData::ChannelAftertouch { channel, pressure }) => {
            Some((0xD0 | (channel & 0x0F), pressure & 0x7F, 0, 2))
        }
        EventBody::Midi(MidiData::PitchBend { channel, value }) => Some((
            0xE0 | (channel & 0x0F),
            (value & 0x7F) as u8,
            ((value >> 7) & 0x7F) as u8,
            3,
        )),
        EventBody::Midi(MidiData::Raw { len, data }) if *len >= 1 && *len <= 3 => Some((
            data[0],
            *data.get(1).unwrap_or(&0),
            *data.get(2).unwrap_or(&0),
            *len as usize,
        )),
        _ => None,
    }
}

/// Inverse of `midi_bytes` — turn 1-3 bytes of LV2 MIDI atom payload
/// into a typed `EventBody`. Anything longer (sysex etc.) is wrapped
/// in `MidiData::Raw` up to the 8-byte cap.
pub(crate) fn decode_midi_atom(bytes: &[u8]) -> Option<EventBody> {
    if bytes.is_empty() {
        return None;
    }
    let status = bytes[0];
    let channel = status & 0x0F;
    let kind = status & 0xF0;
    let body = match (kind, bytes) {
        (0x80, [_, note, vel]) => MidiData::NoteOff {
            channel,
            note: *note,
            velocity: *vel,
        },
        (0x90, [_, note, 0]) => MidiData::NoteOff {
            channel,
            note: *note,
            velocity: 0,
        },
        (0x90, [_, note, vel]) => MidiData::NoteOn {
            channel,
            note: *note,
            velocity: *vel,
        },
        (0xA0, [_, note, pressure]) => MidiData::PolyAftertouch {
            channel,
            note: *note,
            pressure: *pressure,
        },
        (0xB0, [_, controller, value]) => MidiData::ControlChange {
            channel,
            controller: *controller,
            value: *value,
        },
        (0xC0, [_, program]) => MidiData::ProgramChange {
            channel,
            program: *program,
        },
        (0xD0, [_, pressure]) => MidiData::ChannelAftertouch {
            channel,
            pressure: *pressure,
        },
        (0xE0, [_, lsb, msb]) => MidiData::PitchBend {
            channel,
            value: u16::from(*msb) << 7 | u16::from(*lsb),
        },
        _ if bytes.len() <= 8 => {
            let mut data = [0u8; 8];
            data[..bytes.len()].copy_from_slice(bytes);
            #[allow(clippy::cast_possible_truncation)]
            MidiData::Raw {
                len: bytes.len() as u8,
                data,
            }
        }
        _ => return None,
    };
    Some(EventBody::Midi(body))
}
