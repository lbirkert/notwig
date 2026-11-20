// SPDX-License-Identifier: BSD-3-Clause
// VST 2 ABI translated from Xaymar/vst2sdk include/vst.h and
// include/aeffectx.h at 339d4f31590bf77c0d0d248e09a380ac6285e069.
//
// Copyright (c) 2020 Xaymar Dirks <info@xaymar.com> (previously known as Michael Fabian Dirks)
// Copyright 2020 Michael Fabian 'Xaymar' Dirks <info@xaymar.com>
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Clean-room VST 2.4 ABI declarations used by the host side.

#![allow(dead_code)]

use std::ffi::c_void;

pub(crate) const MAGIC: i32 = i32::from_be_bytes(*b"VstP");

pub(crate) const FLAG_HAS_EDITOR: i32 = 1;
pub(crate) const FLAG_CAN_REPLACING: i32 = 1 << 4;
pub(crate) const FLAG_PROGRAM_CHUNKS: i32 = 1 << 5;
pub(crate) const FLAG_IS_SYNTH: i32 = 1 << 8;
pub(crate) const FLAG_IS_EFFECT: i32 = 1 << 9;

pub(crate) const EFF_OPEN: i32 = 0;
pub(crate) const EFF_CLOSE: i32 = 1;
pub(crate) const EFF_SET_PROGRAM: i32 = 2;
pub(crate) const EFF_GET_PROGRAM: i32 = 3;
pub(crate) const EFF_GET_PROGRAM_NAME: i32 = 5;
pub(crate) const EFF_GET_PARAM_LABEL: i32 = 6;
pub(crate) const EFF_GET_PARAM_DISPLAY: i32 = 7;
pub(crate) const EFF_GET_PARAM_NAME: i32 = 8;
pub(crate) const EFF_SET_SAMPLE_RATE: i32 = 10;
pub(crate) const EFF_SET_BLOCK_SIZE: i32 = 11;
pub(crate) const EFF_MAINS_CHANGED: i32 = 12;
pub(crate) const EFF_EDIT_GET_RECT: i32 = 13;
pub(crate) const EFF_EDIT_OPEN: i32 = 14;
pub(crate) const EFF_EDIT_CLOSE: i32 = 15;
pub(crate) const EFF_EDIT_IDLE: i32 = 19;
pub(crate) const EFF_GET_CHUNK: i32 = 23;
pub(crate) const EFF_SET_CHUNK: i32 = 24;
pub(crate) const EFF_PROCESS_EVENTS: i32 = 25;
pub(crate) const EFF_CAN_BE_AUTOMATED: i32 = 26;
pub(crate) const EFF_GET_PROGRAM_NAME_INDEXED: i32 = 29;
pub(crate) const EFF_GET_PLUG_CATEGORY: i32 = 35;
pub(crate) const EFF_GET_EFFECT_NAME: i32 = 45;
pub(crate) const EFF_GET_VENDOR_STRING: i32 = 47;
pub(crate) const EFF_GET_PRODUCT_STRING: i32 = 48;
pub(crate) const EFF_GET_VENDOR_VERSION: i32 = 49;
pub(crate) const EFF_VENDOR_SPECIFIC: i32 = 50;
pub(crate) const EFF_CAN_DO: i32 = 51;
pub(crate) const EFF_GET_TAIL_SAMPLES: i32 = 52;
pub(crate) const EFF_START_PROCESS: i32 = 71;
pub(crate) const EFF_STOP_PROCESS: i32 = 72;
pub(crate) const EFF_SET_PROCESS_PRECISION: i32 = 77;
pub(crate) const EFF_SHELL_GET_NEXT_PLUGIN: i32 = 70;

pub(crate) const PLUG_CATEG_EFFECT: i32 = 1;
pub(crate) const PLUG_CATEG_SYNTH: i32 = 2;
pub(crate) const PLUG_CATEG_SHELL: i32 = 10;

pub(crate) const HOST_AUTOMATE: i32 = 0;
pub(crate) const HOST_VERSION: i32 = 1;
pub(crate) const HOST_CURRENT_ID: i32 = 2;
pub(crate) const HOST_IDLE: i32 = 3;
pub(crate) const HOST_GET_TIME: i32 = 7;
pub(crate) const HOST_PROCESS_EVENTS: i32 = 8;
pub(crate) const HOST_IO_CHANGED: i32 = 13;
pub(crate) const HOST_SIZE_WINDOW: i32 = 15;
pub(crate) const HOST_GET_SAMPLE_RATE: i32 = 16;
pub(crate) const HOST_GET_BLOCK_SIZE: i32 = 17;
pub(crate) const HOST_GET_INPUT_LATENCY: i32 = 18;
pub(crate) const HOST_GET_OUTPUT_LATENCY: i32 = 19;
pub(crate) const HOST_GET_CURRENT_PROCESS_LEVEL: i32 = 23;
pub(crate) const HOST_GET_AUTOMATION_STATE: i32 = 24;
pub(crate) const HOST_GET_VENDOR_STRING: i32 = 32;
pub(crate) const HOST_GET_PRODUCT_STRING: i32 = 33;
pub(crate) const HOST_GET_VENDOR_VERSION: i32 = 34;
pub(crate) const HOST_CAN_DO: i32 = 37;
pub(crate) const HOST_GET_LANGUAGE: i32 = 38;
pub(crate) const HOST_GET_DIRECTORY: i32 = 41;
pub(crate) const HOST_UPDATE_DISPLAY: i32 = 42;
pub(crate) const HOST_BEGIN_EDIT: i32 = 43;
pub(crate) const HOST_END_EDIT: i32 = 44;

pub(crate) const VST_MIDI_TYPE: i32 = 1;
pub(crate) const VST_SYSEX_TYPE: i32 = 6;

pub(crate) const KVST_TRANSPORT_PLAYING: i32 = 1 << 1;
pub(crate) const KVST_TRANSPORT_CYCLE_ACTIVE: i32 = 1 << 2;
pub(crate) const KVST_TRANSPORT_RECORDING: i32 = 1 << 3;
pub(crate) const KVST_PPQ_POS_VALID: i32 = 1 << 9;
pub(crate) const KVST_TEMPO_VALID: i32 = 1 << 10;
pub(crate) const KVST_BARS_VALID: i32 = 1 << 11;
pub(crate) const KVST_CYCLE_POS_VALID: i32 = 1 << 12;
pub(crate) const KVST_TIME_SIG_VALID: i32 = 1 << 13;

pub(crate) type Callback =
    unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
pub(crate) type Process = unsafe extern "C" fn(*mut AEffect, *const *const f32, *mut *mut f32, i32);
pub(crate) type Process64 =
    unsafe extern "C" fn(*mut AEffect, *const *const f64, *mut *mut f64, i32);
pub(crate) type SetParameter = unsafe extern "C" fn(*mut AEffect, u32, f32);
pub(crate) type GetParameter = unsafe extern "C" fn(*mut AEffect, u32) -> f32;
pub(crate) type Entry = unsafe extern "C" fn(Callback) -> *mut AEffect;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct AEffect {
    pub magic: i32,
    pub dispatcher: Option<Callback>,
    pub process: Option<Process>,
    pub set_parameter: Option<SetParameter>,
    pub get_parameter: Option<GetParameter>,
    pub num_programs: i32,
    pub num_params: i32,
    pub num_inputs: i32,
    pub num_outputs: i32,
    pub flags: i32,
    pub resvd1: *mut c_void,
    pub resvd2: *mut c_void,
    pub initial_delay: i32,
    pub real_qualities: i32,
    pub off_qualities: i32,
    pub io_ratio: f32,
    pub object: *mut c_void,
    pub user: *mut c_void,
    pub unique_id: i32,
    pub version: i32,
    pub process_replacing: Option<Process>,
    pub process_double: Option<Process64>,
    pub future: [u8; 56],
}

impl Default for AEffect {
    fn default() -> Self {
        Self {
            magic: MAGIC,
            dispatcher: None,
            process: None,
            set_parameter: None,
            get_parameter: None,
            num_programs: 0,
            num_params: 0,
            num_inputs: 0,
            num_outputs: 0,
            flags: 0,
            resvd1: std::ptr::null_mut(),
            resvd2: std::ptr::null_mut(),
            initial_delay: 0,
            real_qualities: 0,
            off_qualities: 0,
            io_ratio: 1.0,
            object: std::ptr::null_mut(),
            user: std::ptr::null_mut(),
            unique_id: 0,
            version: 0,
            process_replacing: None,
            process_double: None,
            future: [0; 56],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MidiEvent {
    pub kind: i32,
    pub byte_size: i32,
    pub delta_frames: i32,
    pub flags: i32,
    pub note_length: i32,
    pub note_offset: i32,
    pub midi_data: [u8; 4],
    pub detune: i8,
    pub note_off_velocity: u8,
    pub reserved1: u8,
    pub reserved2: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SysExEvent {
    pub kind: i32,
    pub byte_size: i32,
    pub delta_frames: i32,
    pub flags: i32,
    pub dump_bytes: i32,
    pub reserved1: isize,
    pub sysex_dump: *mut u8,
    pub reserved2: isize,
}

impl Default for SysExEvent {
    fn default() -> Self {
        Self {
            kind: VST_SYSEX_TYPE,
            byte_size: (std::mem::size_of::<Self>() - 8) as i32,
            delta_frames: 0,
            flags: 0,
            dump_bytes: 0,
            reserved1: 0,
            sysex_dump: std::ptr::null_mut(),
            reserved2: 0,
        }
    }
}

pub(crate) const MAX_EVENTS: usize = 64;

#[repr(C)]
pub(crate) struct EventList {
    pub num_events: i32,
    pub reserved: isize,
    pub events: [*const c_void; MAX_EVENTS],
}

impl Default for EventList {
    fn default() -> Self {
        Self {
            num_events: 0,
            reserved: 0,
            events: [std::ptr::null(); MAX_EVENTS],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TimeInfo {
    pub sample_position: f64,
    pub sample_rate: f64,
    pub nano_seconds: f64,
    pub ppq_position: f64,
    pub tempo: f64,
    pub bar_start_position: f64,
    pub cycle_start_position: f64,
    pub cycle_end_position: f64,
    pub time_sig_numerator: i32,
    pub time_sig_denominator: i32,
    pub smpte_offset: i32,
    pub smpte_frame_rate: i32,
    pub samples_to_next_clock: i32,
    pub flags: i32,
}

impl TimeInfo {
    pub(crate) const ZERO: Self = Self {
        sample_position: 0.0,
        sample_rate: 0.0,
        nano_seconds: 0.0,
        ppq_position: 0.0,
        tempo: 0.0,
        bar_start_position: 0.0,
        cycle_start_position: 0.0,
        cycle_end_position: 0.0,
        time_sig_numerator: 0,
        time_sig_denominator: 0,
        smpte_offset: 0,
        smpte_frame_rate: 0,
        samples_to_next_clock: 0,
        flags: 0,
    };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}
