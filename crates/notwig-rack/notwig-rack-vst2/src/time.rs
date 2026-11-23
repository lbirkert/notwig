//! Host transport/time state exposed to plugins through `audioMasterGetTime`.

use crate::abi::{self, TimeInfo};
use notwig_rack_core::transport::TransportInfo;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

thread_local! {
    static TIME: UnsafeCell<TimeInfo> = const { UnsafeCell::new(TimeInfo::ZERO) };
}

pub(crate) struct TimeState {
    sample_position: AtomicU64,
    ppq_position: AtomicU64,
    tempo: AtomicU64,
    bar_start: AtomicU64,
    cycle_start: AtomicU64,
    cycle_end: AtomicU64,
    numerator: AtomicU32,
    denominator: AtomicU32,
    flags: AtomicU32,
}

impl Default for TimeState {
    fn default() -> Self {
        Self {
            sample_position: AtomicU64::new(0),
            ppq_position: AtomicU64::new(0),
            tempo: AtomicU64::new(0),
            bar_start: AtomicU64::new(0),
            cycle_start: AtomicU64::new(0),
            cycle_end: AtomicU64::new(0),
            numerator: AtomicU32::new(0),
            denominator: AtomicU32::new(0),
            flags: AtomicU32::new(0),
        }
    }
}

impl TimeState {
    pub(crate) fn update(&self, transport: Option<TransportInfo>) {
        let transport = transport.unwrap_or_default();
        let mut flags = 0;

        store_f64(
            &self.sample_position,
            transport.song_position_samples.unwrap_or(0) as f64,
        );

        if let Some(tempo) = transport.tempo_bpm {
            store_f64(&self.tempo, tempo);
            flags |= abi::KVST_TEMPO_VALID;
        } else {
            store_f64(&self.tempo, 0.0);
        }
        if let Some(beats) = transport.song_position_beats {
            store_f64(&self.ppq_position, beats);
            flags |= abi::KVST_PPQ_POS_VALID;
        } else {
            store_f64(&self.ppq_position, 0.0);
        }
        if let Some(bar) = transport.bar_start_beats {
            store_f64(&self.bar_start, bar);
            flags |= abi::KVST_BARS_VALID;
        } else {
            store_f64(&self.bar_start, 0.0);
        }
        if let Some((numerator, denominator)) = transport.time_signature {
            self.numerator.store(numerator, Ordering::Relaxed);
            self.denominator.store(denominator, Ordering::Relaxed);
            flags |= abi::KVST_TIME_SIG_VALID;
        } else {
            self.numerator.store(0, Ordering::Relaxed);
            self.denominator.store(0, Ordering::Relaxed);
        }

        store_f64(&self.cycle_start, 0.0);
        store_f64(&self.cycle_end, 0.0);

        if transport.playing {
            flags |= abi::KVST_TRANSPORT_PLAYING;
        }
        if transport.recording {
            flags |= abi::KVST_TRANSPORT_RECORDING;
        }
        if transport.loop_active {
            flags |= abi::KVST_TRANSPORT_CYCLE_ACTIVE;
        }
        #[allow(clippy::cast_possible_truncation)]
        self.flags.store(flags as u32, Ordering::Relaxed);
    }

    pub(crate) fn pointer(&self, sample_rate: f64) -> isize {
        let time = TimeInfo {
            sample_position: load_f64(&self.sample_position),
            sample_rate,
            nano_seconds: 0.0,
            ppq_position: load_f64(&self.ppq_position),
            tempo: load_f64(&self.tempo),
            bar_start_position: load_f64(&self.bar_start),
            cycle_start_position: load_f64(&self.cycle_start),
            cycle_end_position: load_f64(&self.cycle_end),
            time_sig_numerator: self.numerator.load(Ordering::Relaxed) as i32,
            time_sig_denominator: self.denominator.load(Ordering::Relaxed) as i32,
            smpte_offset: 0,
            smpte_frame_rate: 0,
            samples_to_next_clock: 0,
            flags: self.flags.load(Ordering::Relaxed) as i32,
        };
        TIME.with(|cell| {
            unsafe { cell.get().write(time) };
            cell.get() as isize
        })
    }
}

fn store_f64(target: &AtomicU64, value: f64) {
    target.store(value.to_bits(), Ordering::Relaxed);
}

pub(crate) fn load_f64(source: &AtomicU64) -> f64 {
    f64::from_bits(source.load(Ordering::Relaxed))
}
