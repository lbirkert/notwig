//! Host-side shared state, callbacks, and event queues for VST 2 plugins.

use crate::abi::{self, AEffect, EventList as VstEventList, MAX_EVENTS, MidiEvent, SysExEvent};
use crate::midi::midi_body;
use crate::time::{TimeState, load_f64};
use crate::{DEFAULT_BLOCK_SIZE, DEFAULT_SAMPLE_RATE};
use crossbeam_queue::ArrayQueue;
use notwig_rack_core::events::{Event, EventBody, MidiData};
use std::cell::Cell;
use std::ffi::{CString, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

thread_local! {
    pub(crate) static CURRENT_ID: Cell<i32> = const { Cell::new(0) };
    pub(crate) static PROCESS_LEVEL: Cell<isize> = const { Cell::new(1) };
}

pub(crate) struct HostShared {
    pub(crate) sample_rate: AtomicU64,
    pub(crate) block_size: AtomicUsize,
    pub(crate) output_events: ArrayQueue<Event>,
    dropped_events: AtomicU32,
    pub(crate) editor_open: AtomicBool,
    pub(crate) editor_size: AtomicU64,
    io_changed: AtomicBool,
    pub(crate) time: TimeState,
    directory: Option<CString>,
}

impl HostShared {
    pub(crate) fn new(directory: Option<CString>) -> Self {
        Self {
            sample_rate: AtomicU64::new(DEFAULT_SAMPLE_RATE.to_bits()),
            block_size: AtomicUsize::new(DEFAULT_BLOCK_SIZE),
            output_events: ArrayQueue::new(MAX_EVENTS),
            dropped_events: AtomicU32::new(0),
            editor_open: AtomicBool::new(false),
            editor_size: AtomicU64::new(0),
            io_changed: AtomicBool::new(false),
            time: TimeState::default(),
            directory,
        }
    }

    pub(crate) fn take_editor_size(&self) -> Option<(u32, u32)> {
        let packed = self.editor_size.swap(0, Ordering::AcqRel);
        if packed == 0 {
            None
        } else {
            Some(((packed >> 32) as u32, packed as u32))
        }
    }
}

pub(crate) struct ProcessLevel(isize);

impl ProcessLevel {
    pub(crate) fn enter() -> Self {
        Self(PROCESS_LEVEL.replace(2))
    }
}

impl Drop for ProcessLevel {
    fn drop(&mut self) {
        PROCESS_LEVEL.set(self.0);
    }
}

fn copy_string(pointer: *mut c_void, text: &[u8]) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let length = text.len().min(63);
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), pointer.cast::<u8>(), length);
        pointer.cast::<u8>().add(length).write(0);
    }
    1
}

fn can_do(pointer: *mut c_void) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let mut bytes = [0u8; 128];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = unsafe { pointer.cast::<u8>().add(index).read() };
        if *byte == 0 {
            return isize::from(matches!(
                &bytes[..index],
                b"sendVstEvents"
                    | b"sendVstMidiEvent"
                    | b"receiveVstEvents"
                    | b"receiveVstMidiEvent"
                    | b"sendVstTimeInfo"
                    | b"sizeWindow"
                    | b"startStopProcess"
                    | b"shellCategory"
            ));
        }
    }
    0
}

pub(crate) unsafe extern "C" fn host_callback(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    pointer: *mut c_void,
    option: f32,
) -> isize {
    match opcode {
        abi::HOST_CURRENT_ID => return CURRENT_ID.with(Cell::get) as isize,
        abi::HOST_VERSION => return 2400,
        abi::HOST_GET_CURRENT_PROCESS_LEVEL => return PROCESS_LEVEL.with(Cell::get),
        abi::HOST_CAN_DO => return can_do(pointer),
        abi::HOST_GET_VENDOR_STRING | abi::HOST_GET_PRODUCT_STRING => {
            return copy_string(pointer, b"notwig-rack");
        }
        abi::HOST_GET_VENDOR_VERSION => return 11_005,
        abi::HOST_GET_LANGUAGE => return 1,
        _ => {}
    }

    if effect.is_null() {
        return match opcode {
            abi::HOST_GET_SAMPLE_RATE => DEFAULT_SAMPLE_RATE as isize,
            abi::HOST_GET_BLOCK_SIZE => DEFAULT_BLOCK_SIZE as isize,
            _ => 0,
        };
    }

    let state = unsafe { (*effect).resvd1.cast::<HostShared>() };
    if state.is_null() {
        return 0;
    }
    let state = unsafe { &*state };

    match opcode {
        abi::HOST_AUTOMATE => {
            if index >= 0 && option.is_finite() {
                push_event(
                    state,
                    Event {
                        sample_offset: 0,
                        body: EventBody::ParamValue {
                            param_id: index as u32,
                            value: f64::from(option),
                        },
                    },
                );
            }
            1
        }
        abi::HOST_GET_TIME => state.time.pointer(load_f64(&state.sample_rate)),
        abi::HOST_PROCESS_EVENTS => unsafe { receive_plugin_events(state, pointer) },
        abi::HOST_IO_CHANGED => {
            state.io_changed.store(true, Ordering::Release);
            1
        }
        abi::HOST_SIZE_WINDOW => {
            if state.editor_open.load(Ordering::Acquire)
                && (1..=8192).contains(&index)
                && (1..=8192).contains(&value)
            {
                state
                    .editor_size
                    .store(((index as u64) << 32) | value as u64, Ordering::Release);
                1
            } else {
                0
            }
        }
        abi::HOST_GET_SAMPLE_RATE => load_f64(&state.sample_rate) as isize,
        abi::HOST_GET_BLOCK_SIZE => state.block_size.load(Ordering::Relaxed) as isize,
        abi::HOST_GET_AUTOMATION_STATE => 1,
        abi::HOST_GET_DIRECTORY => state
            .directory
            .as_ref()
            .map_or(0, |directory| directory.as_ptr() as isize),
        abi::HOST_BEGIN_EDIT => {
            push_gesture(state, index, true);
            1
        }
        abi::HOST_END_EDIT => {
            push_gesture(state, index, false);
            1
        }
        _ => 0,
    }
}

fn push_event(state: &HostShared, event: Event) {
    if state.output_events.push(event).is_err() {
        state.dropped_events.fetch_add(1, Ordering::Relaxed);
    }
}

fn push_gesture(state: &HostShared, index: i32, active: bool) {
    if index < 0 {
        return;
    }
    push_event(
        state,
        Event {
            sample_offset: 0,
            body: EventBody::ParamGesture {
                param_id: index as u32,
                active,
            },
        },
    );
}

#[allow(clippy::cast_ptr_alignment)]
unsafe fn receive_plugin_events(state: &HostShared, pointer: *mut c_void) -> isize {
    if pointer.is_null() {
        return 0;
    }
    let base = pointer.cast::<u8>();
    let count = unsafe { base.cast::<i32>().read_unaligned() };
    if !(0..=MAX_EVENTS as i32).contains(&count) {
        state.dropped_events.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    let events = unsafe {
        base.add(std::mem::offset_of!(VstEventList, events))
            .cast::<*const c_void>()
    };
    for index in 0..count as usize {
        let event = unsafe { events.add(index).read_unaligned() };
        if event.is_null() {
            continue;
        }
        let kind = unsafe { event.cast::<i32>().read() };
        let byte_size = unsafe { event.cast::<i32>().add(1).read() };
        let readable = usize::try_from(byte_size).unwrap_or(0).saturating_add(8);
        match kind {
            abi::VST_MIDI_TYPE => {
                if readable < std::mem::size_of::<MidiEvent>() {
                    state.dropped_events.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let midi = unsafe { event.cast::<MidiEvent>().read() };
                let offset = u32::try_from(midi.delta_frames.max(0)).unwrap_or(0);
                let body = midi_body(midi.midi_data);
                push_event(
                    state,
                    Event {
                        sample_offset: offset,
                        body: EventBody::Midi(body),
                    },
                );
            }
            abi::VST_SYSEX_TYPE => {
                let needed =
                    std::mem::offset_of!(SysExEvent, sysex_dump) + std::mem::size_of::<*mut u8>();
                if readable < needed {
                    state.dropped_events.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let sysex = unsafe { event.cast::<SysExEvent>().read() };
                let length = usize::try_from(sysex.dump_bytes.max(0)).unwrap_or(0).min(8);
                if length == 0 || sysex.sysex_dump.is_null() {
                    continue;
                }
                let mut data = [0u8; 8];
                unsafe {
                    ptr::copy_nonoverlapping(sysex.sysex_dump, data.as_mut_ptr(), length);
                }
                let offset = u32::try_from(sysex.delta_frames.max(0)).unwrap_or(0);
                push_event(
                    state,
                    Event {
                        sample_offset: offset,
                        body: EventBody::Midi(MidiData::Raw {
                            len: length as u8,
                            data,
                        }),
                    },
                );
            }
            _ => {
                state.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    1
}
