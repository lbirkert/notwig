use notwig_core::{Db, TimeMus};
use notwig_dsp::Unit;

pub fn snap_step(pixels_per_beat: f32, beats_per_bar: u8) -> TimeMus {
    let bar = f64::from(beats_per_bar.max(1));
    let candidates = [
        1.0 / 64.0,
        1.0 / 32.0,
        1.0 / 16.0,
        1.0 / 8.0,
        1.0 / 4.0,
        1.0 / 2.0,
        1.0,
        bar,
        bar * 2.0,
        bar * 4.0,
        bar * 8.0,
    ];
    let mut chosen = *candidates.last().expect("grid candidates");
    for step in candidates {
        if (step as f32) * pixels_per_beat >= 6.0 {
            chosen = step;
            break;
        }
    }
    TimeMus::new((chosen * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn snap_time(time: TimeMus, step: TimeMus, bypass: bool) -> TimeMus {
    if bypass || step.is_zero() {
        return time;
    }
    let beats = time.as_f64();
    let step_beats = step.as_f64();
    let snapped = (beats / step_beats).round() * step_beats;
    TimeMus::new((snapped * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn snap_16(time: TimeMus, bypass: bool) -> TimeMus {
    const GRID: f64 = 16.0;
    if bypass {
        return time;
    }
    let beats = time.as_f64();
    let snapped = (beats * GRID).round() / GRID;
    TimeMus::new((snapped * 1_000_000.0).round() as i64, 1_000_000)
}

pub fn format_beats(time: TimeMus, beats_per_bar: u8) -> String {
    let beats = time.as_f64().max(0.0);
    let bar = (beats / f64::from(beats_per_bar.max(1))).floor() as u64 + 1;
    let beat_in_bar = beats % f64::from(beats_per_bar.max(1));
    let beat = beat_in_bar.floor() as u64 + 1;
    let sixteenth = ((beat_in_bar.fract()) * 4.0).floor() as u64 + 1;
    format!("{bar}.{beat}.{sixteenth}")
}

pub fn format_db(value: Db) -> String {
    if value.0 <= -95.0 {
        "-inf".into()
    } else {
        format!("{:+.1} dB", value.0)
    }
}

pub fn format_param(unit: Unit, value: f64) -> String {
    match unit {
        Unit::Decibels => format!("{value:+.2} dB"),
        Unit::Hertz => {
            if value >= 1000.0 {
                format!("{:.2} kHz", value / 1000.0)
            } else {
                format!("{value:.1} Hz")
            }
        }
        Unit::Percent => format!("{:.0}%", value * 100.0),
        Unit::Seconds => format!("{value:.3} s"),
        Unit::Semitones => format!("{value:+.1} st"),
        Unit::None => format!("{value:.2}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_step_adapts_to_zoom() {
        let far = snap_step(16.0, 4);
        let near = snap_step(240.0, 4);
        assert!(far > near, "zoomed out should snap coarser");
        assert_eq!(near, TimeMus::new(1, 32));
    }

    #[test]
    fn snapping_uses_the_step() {
        let step = TimeMus::new(1, 8);
        assert_eq!(
            snap_time(TimeMus::new(3, 16), step, false),
            TimeMus::new(1, 4)
        );
        let raw = TimeMus::new(3, 16);
        assert_eq!(snap_time(raw, step, true), raw);
    }
}
