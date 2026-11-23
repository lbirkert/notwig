use notwig_rack_core::transport::TransportInfo;

use lv2_raw::atom::{LV2Atom, LV2AtomEvent, LV2AtomObjectBody, LV2AtomPropertyBody};
use lv2_raw::urid::LV2Urid;

/// URIDs needed to build a `time:Position` object atom, mapped once
/// at load. The `atom_*` entries are the value types each property
/// carries; the rest are the `time:` properties themselves.
#[derive(Clone, Copy)]
pub(crate) struct TimeUrids {
    pub(crate) position: LV2Urid,
    pub(crate) frame: LV2Urid,
    pub(crate) speed: LV2Urid,
    pub(crate) bar: LV2Urid,
    pub(crate) bar_beat: LV2Urid,
    pub(crate) beats_per_bar: LV2Urid,
    pub(crate) beat_unit: LV2Urid,
    pub(crate) bpm: LV2Urid,
    pub(crate) atom_long: LV2Urid,
    pub(crate) atom_float: LV2Urid,
    pub(crate) atom_int: LV2Urid,
    pub(crate) atom_object: LV2Urid,
}

/// Upper bound on the bytes one `time:Position` event consumes:
/// event + object header plus eight 8-byte-aligned properties. Used
/// to bounds-check once instead of per-property.
pub(crate) const TIME_POSITION_MAX_BYTES: usize =
    std::mem::size_of::<LV2AtomEvent>() + std::mem::size_of::<LV2AtomObjectBody>() + 8 * 32;

/// Write one `time:` property (key + typed scalar value) into an
/// object body at `off`, returning the next 8-byte-aligned offset.
///
/// # Safety
/// `buf + off` must have room for the property header plus `value`
/// rounded up to 8 bytes; callers guarantee this via a single
/// up-front [`TIME_POSITION_MAX_BYTES`] check.
unsafe fn write_property(
    buf: *mut u8,
    off: usize,
    key: LV2Urid,
    value_type: LV2Urid,
    value: &[u8],
) -> usize {
    let prop = unsafe { &mut *buf.add(off).cast::<LV2AtomPropertyBody>() };
    prop.key = key;
    prop.context = 0;
    prop.value = LV2Atom {
        size: u32::try_from(value.len()).unwrap_or(0),
        mytype: value_type,
    };
    let data = unsafe { buf.add(off + std::mem::size_of::<LV2AtomPropertyBody>()) };
    unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), data, value.len()) };
    let total = std::mem::size_of::<LV2AtomPropertyBody>() + value.len();
    off + ((total + 7) & !7)
}

/// Write a `time:Position` object event (at frame 0) into the atom
/// sequence buffer starting at `base`, returning the offset just
/// past it. Returns `base` unchanged if there isn't room.
///
/// # Safety
/// `buf` must point at a buffer of at least `cap` bytes that is
/// valid for writes in `[base, cap)` and aligned for atom structs.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub(crate) unsafe fn write_time_position(
    urids: &TimeUrids,
    buf: *mut u8,
    base: usize,
    cap: usize,
    t: &TransportInfo,
) -> usize {
    if base + TIME_POSITION_MAX_BYTES > cap {
        return base;
    }
    let event_header = std::mem::size_of::<LV2AtomEvent>();
    let obj_body_off = base + event_header;

    // Object body header (id = 0 blank, otype = time:Position).
    let obj_body = unsafe { &mut *buf.add(obj_body_off).cast::<LV2AtomObjectBody>() };
    obj_body.id = 0;
    obj_body.otype = urids.position;

    let mut off = obj_body_off + std::mem::size_of::<LV2AtomObjectBody>();

    // speed: 1.0 while rolling, 0.0 when stopped.
    let speed: f32 = if t.playing { 1.0 } else { 0.0 };
    off = unsafe {
        write_property(
            buf,
            off,
            urids.speed,
            urids.atom_float,
            &speed.to_ne_bytes(),
        )
    };

    if let Some(samples) = t.song_position_samples {
        off = unsafe {
            write_property(
                buf,
                off,
                urids.frame,
                urids.atom_long,
                &samples.to_ne_bytes(),
            )
        };
    }
    if let Some(tempo) = t.tempo_bpm {
        let bpm = tempo as f32;
        off = unsafe { write_property(buf, off, urids.bpm, urids.atom_float, &bpm.to_ne_bytes()) };
    }
    if let Some((num, den)) = t.time_signature {
        let beats_per_bar = num as f32;
        off = unsafe {
            write_property(
                buf,
                off,
                urids.beats_per_bar,
                urids.atom_float,
                &beats_per_bar.to_ne_bytes(),
            )
        };
        let beat_unit = den as i32;
        off = unsafe {
            write_property(
                buf,
                off,
                urids.beat_unit,
                urids.atom_int,
                &beat_unit.to_ne_bytes(),
            )
        };

        // Bar / barBeat need the musical position too.
        let beats_per_bar_qn = f64::from(num) * 4.0 / f64::from(den.max(1));
        if let Some(bar_start) = t.bar_start_beats {
            let bar = (bar_start / beats_per_bar_qn.max(f64::EPSILON)).round() as i64;
            off =
                unsafe { write_property(buf, off, urids.bar, urids.atom_long, &bar.to_ne_bytes()) };

            if let Some(beats) = t.song_position_beats {
                // Quarter notes since the bar, expressed in this
                // signature's beats (beatUnit per whole note).
                let bar_beat = ((beats - bar_start) * f64::from(den) / 4.0) as f32;
                off = unsafe {
                    write_property(
                        buf,
                        off,
                        urids.bar_beat,
                        urids.atom_float,
                        &bar_beat.to_ne_bytes(),
                    )
                };
            }
        }
    }

    // Back-patch the event header now that the object size is known.
    let object_body_size = off - obj_body_off;
    let event = unsafe { &mut *buf.add(base).cast::<LV2AtomEvent>() };
    event.time_in_frames = 0;
    event.body = LV2Atom {
        size: object_body_size as u32,
        mytype: urids.atom_object,
    };

    // Pad the whole event to the sequence's 8-byte event alignment.
    (off + 7) & !7
}
