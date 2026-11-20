use serde::{Deserialize, Serialize};

/// Bytes used by one encoded note event on the extension ABI.
pub const NOTE_EVENT_BYTES: usize = 12;

/// Whether a note event starts or stops a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind {
    Off,
    On,
}

/// A note event delivered to an extension device before it processes a block.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NoteEvent {
    /// Sample offset within the current block.
    pub sample_offset: u32,
    /// Whether this is a note-on or note-off.
    pub kind: NoteKind,
    /// MIDI pitch number.
    pub pitch: u8,
    /// Normalized note velocity.
    pub velocity: f32,
}

impl From<notwig_dsp::NoteEvent> for NoteEvent {
    fn from(event: notwig_dsp::NoteEvent) -> Self {
        Self {
            sample_offset: event.sample_offset,
            kind: match event.kind {
                notwig_dsp::NoteKind::On => NoteKind::On,
                notwig_dsp::NoteKind::Off => NoteKind::Off,
            },
            pitch: event.pitch,
            velocity: event.velocity,
        }
    }
}

/// Encodes a note event into the fixed-size extension wire format.
#[must_use]
pub fn encode_note_event(event: &NoteEvent) -> [u8; NOTE_EVENT_BYTES] {
    let mut bytes = [0_u8; NOTE_EVENT_BYTES];
    bytes[0..4].copy_from_slice(&event.sample_offset.to_le_bytes());
    bytes[4] = match event.kind {
        NoteKind::Off => 0,
        NoteKind::On => 1,
    };
    bytes[5] = event.pitch;
    bytes[8..12].copy_from_slice(&event.velocity.to_le_bytes());
    bytes
}

/// Decodes one fixed-size note event, returning `None` for invalid data.
#[must_use]
pub fn decode_note_event(bytes: &[u8]) -> Option<NoteEvent> {
    if bytes.len() < NOTE_EVENT_BYTES {
        return None;
    }
    let sample_offset = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let kind = match bytes[4] {
        0 => NoteKind::Off,
        1 => NoteKind::On,
        _ => return None,
    };
    let pitch = bytes[5];
    let velocity = f32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let event = NoteEvent {
        sample_offset,
        kind,
        pitch,
        velocity,
    };
    event.velocity.is_finite().then_some(event)
}
