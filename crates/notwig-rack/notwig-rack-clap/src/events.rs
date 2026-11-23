//! Conversion between notwig-rack events and CLAP event structs.
//!
//! We can't ship `*const clap_event_header` directly off our `EventList`
//! because the notwig-rack event payloads have different layouts than CLAP's.
//! The fix is to translate up front and keep the result alive for the `process`
//! call.

use notwig_rack_core::events::EventList;

use clap_sys::events::{
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_MIDI, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_ON,
    CLAP_EVENT_PARAM_VALUE, clap_event_header, clap_event_midi, clap_event_note,
    clap_event_param_value, clap_input_events, clap_output_events,
};

use std::ptr;

fn make_param_value_event(sample_offset: u32, param_id: u32, value: f64) -> clap_event_param_value {
    clap_event_param_value {
        header: clap_event_header {
            size: u32::try_from(std::mem::size_of::<clap_event_param_value>()).unwrap_or(0),
            time: sample_offset,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_PARAM_VALUE,
            flags: 0,
        },
        param_id,
        cookie: ptr::null_mut(),
        note_id: -1,
        port_index: -1,
        channel: -1,
        key: -1,
        value,
    }
}

fn make_note_event(
    sample_offset: u32,
    event_type: u16,
    channel: u8,
    key: u8,
    velocity: f64,
) -> clap_event_note {
    clap_event_note {
        header: clap_event_header {
            size: u32::try_from(std::mem::size_of::<clap_event_note>()).unwrap_or(0),
            time: sample_offset,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: event_type,
            flags: 0,
        },
        note_id: -1,
        port_index: -1,
        channel: i16::from(channel),
        key: i16::from(key),
        velocity,
    }
}

fn make_midi_event(sample_offset: u32, bytes: [u8; 3]) -> clap_event_midi {
    clap_event_midi {
        header: clap_event_header {
            size: u32::try_from(std::mem::size_of::<clap_event_midi>()).unwrap_or(0),
            time: sample_offset,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_MIDI,
            flags: 0,
        },
        port_index: 0,
        data: bytes,
    }
}

/// Owned storage for one block's CLAP-formatted input events.
///
/// We can't ship `*const clap_event_header` directly off our
/// `EventList` because the notwig-rack event payloads have different
/// layouts than CLAP's. The fix is to translate up front and keep
/// the result alive for the `process` call.
#[derive(Default)]
pub(crate) struct ConvertedInputEvents {
    /// Owned event memory — every entry is a CLAP event struct
    /// the input-events vtable hands out pointers into. Boxed so
    /// addresses survive `Vec` reallocation.
    pub(crate) events: Vec<EventStorage>,
}

#[allow(dead_code)]
pub(crate) enum EventStorage {
    Param(clap_event_param_value),
    Note(clap_event_note),
    Midi(clap_event_midi),
}

impl EventStorage {
    fn header(&self) -> *const clap_event_header {
        match self {
            Self::Param(e) => &raw const e.header,
            Self::Note(e) => &raw const e.header,
            Self::Midi(e) => &raw const e.header,
        }
    }
}

impl ConvertedInputEvents {
    /// Reuse the backing event vector for a new block. Keeps capacity so the
    /// audio thread does not allocate once the steady-state event count has
    /// grown into place.
    pub(crate) fn clear(&mut self) {
        self.events.clear();
    }

    /// Translate `list` into CLAP events, appending to the existing storage.
    pub(crate) fn extend_rack_events(&mut self, list: &EventList) {
        for event in list {
            self.push_rack(event);
        }
    }

    pub(crate) fn push_param_value(&mut self, sample_offset: u32, param_id: u32, value: f64) {
        self.events.push(EventStorage::Param(make_param_value_event(
            sample_offset,
            param_id,
            value,
        )));
    }

    fn push_rack(&mut self, event: &notwig_rack_core::events::Event) {
        use notwig_rack_core::events::EventBody;
        let offset = event.sample_offset;
        match event.body {
            EventBody::Midi(midi) => self.push_midi(offset, midi),
            EventBody::ParamValue { param_id, value } => {
                self.push_param_value(offset, param_id, value);
            }
            EventBody::ParamGesture { .. } | EventBody::TransportFlag(_) => {
                // ParamGesture: CLAP's begin/end events have
                // separate types; covered in a follow-on once
                // hosts start emitting them.
                // TransportFlag: routed through clap_event_transport
                // on the process struct, not via input events.
            }
        }
    }

    fn push_midi(&mut self, offset: u32, midi: notwig_rack_core::events::MidiData) {
        use notwig_rack_core::events::MidiData;
        match midi {
            MidiData::NoteOn {
                channel,
                note,
                velocity,
            } => {
                self.events.push(EventStorage::Note(make_note_event(
                    offset,
                    CLAP_EVENT_NOTE_ON,
                    channel,
                    note,
                    f64::from(velocity) / 127.0,
                )));
            }
            MidiData::NoteOff {
                channel,
                note,
                velocity,
            } => {
                self.events.push(EventStorage::Note(make_note_event(
                    offset,
                    CLAP_EVENT_NOTE_OFF,
                    channel,
                    note,
                    f64::from(velocity) / 127.0,
                )));
            }
            MidiData::ControlChange {
                channel,
                controller,
                value,
            } => {
                let status = 0xB0 | (channel & 0x0F);
                self.events.push(EventStorage::Midi(make_midi_event(
                    offset,
                    [status, controller & 0x7F, value & 0x7F],
                )));
            }
            MidiData::ProgramChange { channel, program } => {
                let status = 0xC0 | (channel & 0x0F);
                self.events.push(EventStorage::Midi(make_midi_event(
                    offset,
                    [status, program & 0x7F, 0],
                )));
            }
            MidiData::PolyAftertouch {
                channel,
                note,
                pressure,
            } => {
                let status = 0xA0 | (channel & 0x0F);
                self.events.push(EventStorage::Midi(make_midi_event(
                    offset,
                    [status, note & 0x7F, pressure & 0x7F],
                )));
            }
            MidiData::ChannelAftertouch { channel, pressure } => {
                let status = 0xD0 | (channel & 0x0F);
                self.events.push(EventStorage::Midi(make_midi_event(
                    offset,
                    [status, pressure & 0x7F, 0],
                )));
            }
            MidiData::PitchBend { channel, value } => {
                let status = 0xE0 | (channel & 0x0F);
                let lsb = u8::try_from(value & 0x7F).unwrap_or(0);
                let msb = u8::try_from((value >> 7) & 0x7F).unwrap_or(0);
                self.events.push(EventStorage::Midi(make_midi_event(
                    offset,
                    [status, lsb, msb],
                )));
            }
            MidiData::Raw { len, data } => {
                if len >= 3 {
                    self.events.push(EventStorage::Midi(make_midi_event(
                        offset,
                        [data[0], data[1], data[2]],
                    )));
                }
            }
        }
    }

    pub(crate) fn as_clap(&self) -> clap_input_events {
        clap_input_events {
            ctx: std::ptr::from_ref::<Self>(self)
                .cast::<std::ffi::c_void>()
                .cast_mut(),
            size: Some(input_events_size),
            get: Some(input_events_get),
        }
    }
}

unsafe extern "C" fn input_events_size(list: *const clap_input_events) -> u32 {
    let ctx = unsafe { (*list).ctx.cast::<ConvertedInputEvents>() };
    if ctx.is_null() {
        return 0;
    }
    u32::try_from(unsafe { (*ctx).events.len() }).unwrap_or(u32::MAX)
}

unsafe extern "C" fn input_events_get(
    list: *const clap_input_events,
    index: u32,
) -> *const clap_event_header {
    let ctx = unsafe { (*list).ctx.cast::<ConvertedInputEvents>() };
    if ctx.is_null() {
        return ptr::null();
    }
    let idx = index as usize;
    let events = unsafe { &(*ctx).events };
    events.get(idx).map_or(ptr::null(), EventStorage::header)
}

/// Sink for events the plugin emits during `process`.
pub(crate) struct OutputEventsSink<'a> {
    target: Option<&'a mut EventList>,
}

impl<'a> OutputEventsSink<'a> {
    pub(crate) fn new(target: Option<&'a mut EventList>) -> Self {
        Self { target }
    }

    pub(crate) fn as_clap(&mut self) -> clap_output_events {
        clap_output_events {
            ctx: std::ptr::from_mut::<Self>(self).cast::<std::ffi::c_void>(),
            try_push: Some(output_events_try_push),
        }
    }
}

unsafe extern "C" fn output_events_try_push(
    list: *const clap_output_events,
    event: *const clap_event_header,
) -> bool {
    if event.is_null() {
        return false;
    }
    let header = unsafe { &*event };
    let ctx = unsafe { (*list).ctx.cast::<OutputEventsSink<'_>>() };
    if ctx.is_null() {
        return true;
    }
    let target = unsafe { (*ctx).target.as_deref_mut() };
    let Some(target) = target else {
        // Plugin wanted to send an event; the host doesn't care
        // (Sink was constructed with `target: None`). Returning
        // true tells the plugin we accepted it.
        return true;
    };
    if let Some(rack_event) = unsafe { clap_event_to_rack(header) } {
        target.push(rack_event);
    }
    true
}

unsafe fn clap_event_to_rack(
    header: &clap_event_header,
) -> Option<notwig_rack_core::events::Event> {
    use notwig_rack_core::events::{Event, EventBody, MidiData};
    if header.space_id != CLAP_CORE_EVENT_SPACE_ID {
        return None;
    }
    let offset = header.time;
    match header.type_ {
        t if t == CLAP_EVENT_PARAM_VALUE => {
            let e: &clap_event_param_value =
                unsafe { &*std::ptr::from_ref::<clap_event_header>(header).cast() };
            Some(Event {
                sample_offset: offset,
                body: EventBody::ParamValue {
                    param_id: e.param_id,
                    value: e.value,
                },
            })
        }
        t if t == CLAP_EVENT_NOTE_ON => {
            let e: &clap_event_note =
                unsafe { &*std::ptr::from_ref::<clap_event_header>(header).cast() };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let velocity = (e.velocity * 127.0).round().clamp(0.0, 127.0) as u8;
            Some(Event {
                sample_offset: offset,
                body: EventBody::Midi(MidiData::NoteOn {
                    channel: u8::try_from(e.channel.max(0)).unwrap_or(0),
                    note: u8::try_from(e.key.max(0)).unwrap_or(0),
                    velocity,
                }),
            })
        }
        t if t == CLAP_EVENT_NOTE_OFF => {
            let e: &clap_event_note =
                unsafe { &*std::ptr::from_ref::<clap_event_header>(header).cast() };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let velocity = (e.velocity * 127.0).round().clamp(0.0, 127.0) as u8;
            Some(Event {
                sample_offset: offset,
                body: EventBody::Midi(MidiData::NoteOff {
                    channel: u8::try_from(e.channel.max(0)).unwrap_or(0),
                    note: u8::try_from(e.key.max(0)).unwrap_or(0),
                    velocity,
                }),
            })
        }
        t if t == CLAP_EVENT_MIDI => {
            let e: &clap_event_midi =
                unsafe { &*std::ptr::from_ref::<clap_event_header>(header).cast() };
            Some(Event {
                sample_offset: offset,
                body: EventBody::Midi(MidiData::Raw {
                    len: 3,
                    data: [e.data[0], e.data[1], e.data[2], 0, 0, 0, 0, 0],
                }),
            })
        }
        _ => None,
    }
}
