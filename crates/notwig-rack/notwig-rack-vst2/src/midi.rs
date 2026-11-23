//! MIDI and `SysEx` codec between the core event types and the VST 2 ABI.

use crate::abi::{self, EventList as VstEventList, MAX_EVENTS, MidiEvent, SysExEvent};
use notwig_rack_core::events::MidiData;

pub(crate) struct RawMidi {
    pub(crate) len: u8,
    pub(crate) data: [u8; 8],
}

pub(crate) fn midi_to_bytes(midi: MidiData) -> Option<RawMidi> {
    let (length, data): (u8, [u8; 8]) = match midi {
        MidiData::NoteOn {
            channel,
            note,
            velocity,
        } => (3, [0x90 | (channel & 0x0F), note, velocity, 0, 0, 0, 0, 0]),
        MidiData::NoteOff {
            channel,
            note,
            velocity,
        } => (3, [0x80 | (channel & 0x0F), note, velocity, 0, 0, 0, 0, 0]),
        MidiData::PolyAftertouch {
            channel,
            note,
            pressure,
        } => (3, [0xA0 | (channel & 0x0F), note, pressure, 0, 0, 0, 0, 0]),
        MidiData::ControlChange {
            channel,
            controller,
            value,
        } => (
            3,
            [0xB0 | (channel & 0x0F), controller, value, 0, 0, 0, 0, 0],
        ),
        MidiData::ProgramChange { channel, program } => {
            (2, [0xC0 | (channel & 0x0F), program, 0, 0, 0, 0, 0, 0])
        }
        MidiData::ChannelAftertouch { channel, pressure } => {
            (2, [0xD0 | (channel & 0x0F), pressure, 0, 0, 0, 0, 0, 0])
        }
        MidiData::PitchBend { channel, value } => (
            3,
            [
                0xE0 | (channel & 0x0F),
                (value & 0x7F) as u8,
                ((value >> 7) & 0x7F) as u8,
                0,
                0,
                0,
                0,
                0,
            ],
        ),
        MidiData::Raw { len, data } => (len.min(8), data),
    };
    if length == 0 {
        return None;
    }
    Some(RawMidi { len: length, data })
}

pub(crate) fn midi_body(bytes: [u8; 4]) -> MidiData {
    let status = bytes[0];
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x80 => MidiData::NoteOff {
            channel,
            note: bytes[1],
            velocity: bytes[2],
        },
        0x90 => {
            if bytes[2] == 0 {
                MidiData::NoteOff {
                    channel,
                    note: bytes[1],
                    velocity: 0,
                }
            } else {
                MidiData::NoteOn {
                    channel,
                    note: bytes[1],
                    velocity: bytes[2],
                }
            }
        }
        0xA0 => MidiData::PolyAftertouch {
            channel,
            note: bytes[1],
            pressure: bytes[2],
        },
        0xB0 => MidiData::ControlChange {
            channel,
            controller: bytes[1],
            value: bytes[2],
        },
        0xC0 => MidiData::ProgramChange {
            channel,
            program: bytes[1],
        },
        0xD0 => MidiData::ChannelAftertouch {
            channel,
            pressure: bytes[1],
        },
        0xE0 => MidiData::PitchBend {
            channel,
            value: u16::from(bytes[1]) | (u16::from(bytes[2]) << 7),
        },
        _ => MidiData::Raw {
            len: 3,
            data: [bytes[0], bytes[1], bytes[2], 0, 0, 0, 0, 0],
        },
    }
}

pub(crate) fn push_midi_event(
    list: &mut VstEventList,
    events: &mut [MidiEvent; MAX_EVENTS],
    offset: u32,
    bytes: &[u8; 8],
) {
    let index = list.num_events as usize;
    if index >= MAX_EVENTS {
        return;
    }
    let mut data = [0u8; 4];
    data.copy_from_slice(&bytes[..4]);
    events[index] = MidiEvent {
        kind: abi::VST_MIDI_TYPE,
        byte_size: (std::mem::size_of::<MidiEvent>() - 8) as i32,
        delta_frames: i32::try_from(offset).unwrap_or(i32::MAX),
        flags: 0,
        note_length: 0,
        note_offset: 0,
        midi_data: data,
        detune: 0,
        note_off_velocity: 0,
        reserved1: 0,
        reserved2: 0,
    };
    list.events[index] = (&raw const events[index]).cast();
    list.num_events += 1;
}

pub(crate) fn push_sysex_event(
    list: &mut VstEventList,
    events: &mut [SysExEvent; MAX_EVENTS],
    bytes: &mut [[u8; 8]; MAX_EVENTS],
    offset: u32,
    data: &[u8],
) {
    let index = list.num_events as usize;
    if index >= MAX_EVENTS {
        return;
    }
    let length = data.len().min(8);
    bytes[index][..length].copy_from_slice(&data[..length]);
    events[index] = SysExEvent {
        kind: abi::VST_SYSEX_TYPE,
        byte_size: (std::mem::size_of::<SysExEvent>() - 8) as i32,
        delta_frames: i32::try_from(offset).unwrap_or(i32::MAX),
        flags: 0,
        dump_bytes: length as i32,
        reserved1: 0,
        sysex_dump: bytes[index].as_mut_ptr(),
        reserved2: 0,
    };
    list.events[index] = (&raw const events[index]).cast();
    list.num_events += 1;
}
