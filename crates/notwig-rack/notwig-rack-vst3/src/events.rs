//! In-memory `IEventList` implementation for MIDI in/out.

use vst3::Class;
use vst3::Steinberg::Vst::{
    Event, Event_::EventTypes_, Event__type0, IEventList, IEventListTrait, NoteOffEvent,
    NoteOnEvent, PolyPressureEvent,
};
use vst3::Steinberg::kResultOk;

/// Translation buffer for notwig-rack-core `EventList` → VST3 `Event[]`.
#[derive(Default)]
pub(crate) struct EventBuffer {
    pub(crate) events: Vec<Event>,
}

impl EventBuffer {
    pub(crate) fn push_rack(&mut self, event: &notwig_rack_core::events::Event) {
        use notwig_rack_core::events::{EventBody, MidiData};
        let offset = i32::try_from(event.sample_offset).unwrap_or(i32::MAX);
        let EventBody::Midi(body) = event.body else {
            return;
        };
        let header = |type_: u16| Event {
            busIndex: 0,
            sampleOffset: offset,
            ppqPosition: 0.0,
            flags: 0,
            r#type: type_,
            __field0: Event__type0 {
                noteOn: NoteOnEvent {
                    channel: 0,
                    pitch: 0,
                    tuning: 0.0,
                    velocity: 0.0,
                    length: 0,
                    noteId: -1,
                },
            },
        };
        match body {
            MidiData::NoteOn {
                channel,
                note,
                velocity,
            } => {
                let mut ev = header(u16::try_from(EventTypes_::kNoteOnEvent).unwrap_or(0));
                ev.__field0 = Event__type0 {
                    noteOn: NoteOnEvent {
                        channel: i16::from(channel),
                        pitch: i16::from(note),
                        tuning: 0.0,
                        velocity: f32::from(velocity) / 127.0,
                        length: 0,
                        noteId: -1,
                    },
                };
                self.events.push(ev);
            }
            MidiData::NoteOff {
                channel,
                note,
                velocity,
            } => {
                let mut ev = header(u16::try_from(EventTypes_::kNoteOffEvent).unwrap_or(0));
                ev.__field0 = Event__type0 {
                    noteOff: NoteOffEvent {
                        channel: i16::from(channel),
                        pitch: i16::from(note),
                        velocity: f32::from(velocity) / 127.0,
                        noteId: -1,
                        tuning: 0.0,
                    },
                };
                self.events.push(ev);
            }
            MidiData::PolyAftertouch {
                channel,
                note,
                pressure,
            } => {
                let mut ev = header(u16::try_from(EventTypes_::kPolyPressureEvent).unwrap_or(0));
                ev.__field0 = Event__type0 {
                    polyPressure: PolyPressureEvent {
                        channel: i16::from(channel),
                        pitch: i16::from(note),
                        pressure: f32::from(pressure) / 127.0,
                        noteId: -1,
                    },
                };
                self.events.push(ev);
            }
            // VST3 routes CC / ProgramChange / ChannelAftertouch /
            // PitchBend / Sysex through IParameterChanges or
            // IMidiMapping rather than IEventList. Wiring those is
            // a follow-on; for hosting test purposes notes are the
            // primary need.
            _ => {}
        }
    }
}

#[derive(Default)]
pub(crate) struct EventList3 {
    pub(crate) events: std::cell::RefCell<Vec<Event>>,
}

impl Class for EventList3 {
    type Interfaces = (IEventList,);
}

#[allow(clippy::cast_sign_loss)]
impl IEventListTrait for EventList3 {
    unsafe fn getEventCount(&self) -> i32 {
        i32::try_from(self.events.borrow().len()).unwrap_or(i32::MAX)
    }

    unsafe fn getEvent(&self, index: i32, out: *mut Event) -> i32 {
        if out.is_null() || index < 0 {
            return -1;
        }
        let events = self.events.borrow();
        let Some(event) = events.get(index as usize) else {
            return -1;
        };
        unsafe { *out = *event };
        kResultOk
    }

    unsafe fn addEvent(&self, event: *mut Event) -> i32 {
        if event.is_null() {
            return -1;
        }
        self.events.borrow_mut().push(unsafe { *event });
        kResultOk
    }
}
